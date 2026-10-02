use crate::error::NodeError;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use jsonrpc::client::{Client, Transport};
use jsonrpc::{Request, Response};

use crate::{RpcAuth, RpcConfig};

/// Result type for node operations.
pub type Result<T> = std::result::Result<T, NodeError>;

/// Response from `node status`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeStatus {
    pub chain: String,
    pub blocks: u32,
    pub headers: u32,
    pub sync_percentage: f64,
    pub connections: u32,
}

/// Response from `block info`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BlockInfo {
    pub hash: String,
    pub height: u32,
    pub time: u64,
    pub tx_count: usize,
    pub size: usize,
    pub version: u32,
    pub previous_block_hash: Option<String>,
    pub merkle_root: String,
    pub bits: String,
    pub difficulty: f64,
}

/// Response from `fee estimate`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FeeEstimate {
    pub sat_vb: f64,
    pub mode: String,
    pub target_blocks: u16,
    pub is_fallback: bool,
}

/// A transaction fetched from a node, with the amounts of the outputs its inputs spend when
/// the node can say (Bitcoin Core 24+, `getrawtransaction` verbosity 2).
#[derive(Debug, Clone)]
pub struct FetchedTx {
    pub hex: String,
    /// `(sats, scriptPubKey hex)` per input, in order. `None` for coinbases or unknown.
    pub prevouts: Option<Vec<(u64, String)>>,
}

/// Trait for accessing a Bitcoin Core node. Implemented by RPC and mock backends.
pub trait NodeBackend: Send + Sync {
    fn node_status(&self) -> Result<NodeStatus>;
    fn block_info(&self, height_or_hash: &str) -> Result<BlockInfo>;
    fn fee_estimate(&self, target: u16, mode: &str) -> Result<FeeEstimate>;

    /// The chain the node is on (`main`, `regtest`, `signet`, `testnet4`). Cheaper than `node_status` when only that is needed.
    fn chain(&self) -> Result<String> {
        Ok(self.node_status()?.chain)
    }

    /// The lowest fee rate (sat/vB) the node's mempool accepts. A floor, not a confirmation estimate;
    /// used when the node cannot estimate fees.
    fn mempool_floor_sat_vb(&self) -> Result<f64> {
        Err(NodeError::Rpc(
            "this backend cannot report the mempool minimum fee".into(),
        ))
    }

    /// The raw hex of a transaction. Without a transaction index the node only knows unconfirmed
    /// transactions, unless `block` (a height or block hash) says where to look.
    fn raw_transaction(&self, _txid: &str, _block: Option<&str>) -> Result<String> {
        Err(NodeError::Rpc(
            "this backend cannot fetch transactions".into(),
        ))
    }

    /// Like `raw_transaction`, plus the spent amounts when the node provides them.
    fn transaction(&self, txid: &str, block: Option<&str>) -> Result<FetchedTx> {
        Ok(FetchedTx {
            hex: self.raw_transaction(txid, block)?,
            prevouts: None,
        })
    }

    /// `(sat/vB, vsize)` for every non-coinbase transaction in the last `blocks` blocks, for
    /// estimating fees when the node's own estimator is unavailable.
    fn recent_fee_rates(&self, _blocks: usize) -> Result<Vec<(f64, u64)>> {
        Err(NodeError::Rpc(
            "this backend cannot read recent blocks".into(),
        ))
    }

    /// Submits a signed transaction (hex) and returns its txid.
    fn send_raw_transaction(&self, hex: &str) -> Result<String>;
}

/// Real implementation using Bitcoin Core's JSON-RPC, locally (cookie or user/password)
/// or through a hosted gateway (`X-API-Key`).
///
/// The client is built on every call rather than at startup: offline commands never
/// need the node, and Bitcoin Core rewrites its cookie file on each restart.
pub struct CoreRpcBackend {
    url: String,
    auth: Credentials,
    /// Recent blocks are ~10 MB each to download, so keep what was read for a while.
    fee_cache: std::sync::Mutex<Option<FeeCache>>,
}

struct FeeCache {
    taken: std::time::Instant,
    tip: u64,
    blocks: usize,
    samples: Vec<(f64, u64)>,
}

const FEE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(600);

enum Credentials {
    Cookie(std::path::PathBuf),
    UserPass(String, String),
    ApiKey(String),
}

/// A non-200 reply. Kept distinct so a rejected key reads differently from a dead node.
#[derive(Debug)]
struct HttpFailure {
    status: u16,
    body: String,
}

impl fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.body.trim())
    }
}

impl std::error::Error for HttpFailure {}

struct HttpTransport {
    url: String,
    headers: Vec<(&'static str, String)>,
}

impl HttpTransport {
    fn post<R: serde::de::DeserializeOwned>(
        &self,
        body: &impl serde::Serialize,
    ) -> std::result::Result<R, jsonrpc::Error> {
        let transport = |e: Box<dyn std::error::Error + Send + Sync>| jsonrpc::Error::Transport(e);
        let mut request = bitreq::Request::new(bitreq::Method::Post, &self.url).with_timeout(60);
        for (name, value) in &self.headers {
            request = request.with_header(*name, value.as_str());
        }
        let response = request
            .with_json(body)
            .map_err(|e| transport(Box::new(e)))?
            .send()
            .map_err(|e| transport(Box::new(e)))?;
        if response.status_code != 200 {
            // Bitcoin Core reports RPC errors as HTTP 4xx/5xx *with* a JSON-RPC body, so try that first.
            if let Ok(parsed) = response.json::<R>() {
                return Ok(parsed);
            }
            return Err(transport(Box::new(HttpFailure {
                status: response.status_code as u16,
                body: response.as_str().unwrap_or("").chars().take(300).collect(),
            })));
        }
        response.json().map_err(|e| transport(Box::new(e)))
    }
}

impl Transport for HttpTransport {
    fn send_request(&self, req: Request) -> std::result::Result<Response, jsonrpc::Error> {
        self.post(&req)
    }

    fn send_batch(&self, reqs: &[Request]) -> std::result::Result<Vec<Response>, jsonrpc::Error> {
        self.post(&reqs)
    }

    fn fmt_target(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl CoreRpcBackend {
    pub fn new(config: &RpcConfig) -> Result<Self> {
        let auth = match &config.auth {
            RpcAuth::Cookie(path) => Credentials::Cookie(path.clone()),
            RpcAuth::UserPass { user, password } => {
                Credentials::UserPass(user.clone(), password.to_string())
            }
            RpcAuth::ApiKey(key) => Credentials::ApiKey(key.to_string()),
        };
        Ok(CoreRpcBackend {
            url: config.url.clone(),
            auth,
            fee_cache: std::sync::Mutex::new(None),
        })
    }

    fn client(&self) -> Result<Client> {
        let basic =
            |user: &str, pass: &str| format!("Basic {}", BASE64.encode(format!("{user}:{pass}")));
        let headers = match &self.auth {
            Credentials::Cookie(path) => {
                let line = std::fs::read_to_string(path).map_err(|e| {
                    self.unreachable(format!(
                        "cannot read cookie file {} ({e}) - is bitcoind running on this network? \
                         Set BTC_RPC_COOKIE, BTC_RPC_USER and BTC_RPC_PASSWORD, or BTC_RPC_API_KEY",
                        path.display()
                    ))
                })?;
                let line = line.lines().next().unwrap_or("");
                let (user, pass) = line.split_once(':').ok_or_else(|| {
                    NodeError::Rpc(format!("cookie file {} is malformed", path.display()))
                })?;
                vec![("Authorization", basic(user, pass))]
            }
            Credentials::UserPass(user, pass) => vec![("Authorization", basic(user, pass))],
            Credentials::ApiKey(key) => vec![("X-API-Key", key.clone())],
        };
        Ok(Client::with_transport(HttpTransport {
            url: self.url.clone(),
            headers,
        }))
    }

    fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: &[serde_json::Value],
    ) -> Result<T> {
        let client = self.client()?;
        let raw = serde_json::value::to_raw_value(args)
            .map_err(|e| NodeError::Rpc(format!("{method}: bad arguments: {e}")))?;
        let response = client
            .send_request(client.build_request(method, Some(&raw)))
            .map_err(|e| self.classify(method, e))?;
        response.result().map_err(|e| self.classify(method, e))
    }

    fn classify(&self, method: &str, err: jsonrpc::Error) -> NodeError {
        match err {
            jsonrpc::Error::Rpc(rpc) => {
                NodeError::Rpc(format!("{method}: {} (code {})", rpc.message, rpc.code))
            }
            jsonrpc::Error::Transport(inner) => match inner.downcast_ref::<HttpFailure>() {
                Some(http) if http.status == 401 || http.status == 403 => NodeError::Rpc(format!(
                    "{method}: the node rejected the request ({http}). Check BTC_RPC_API_KEY (or your cookie / \
                     user-password). Hosted gateways also only allow some methods"
                )),
                Some(http) if http.body.contains("not permitted") => NodeError::Rpc(format!(
                    "{method} is not allowed by this node gateway ({http}). \
                     For fees, pass --fallback-rate <sat/vB>"
                )),
                Some(http) => NodeError::Rpc(format!("{method}: {http}")),
                None => {
                    self.unreachable(format!("{inner} - is the node running? Check BTC_RPC_URL"))
                }
            },
            other => NodeError::Rpc(format!("{method}: {other}")),
        }
    }

    /// A block hash from a height, or the hash itself.
    fn block_hash(&self, height_or_hash: &str) -> Result<String> {
        if height_or_hash.chars().all(|c| c.is_ascii_digit()) {
            let height: u64 = height_or_hash
                .parse()
                .map_err(|_| NodeError::Rpc(format!("invalid block height {height_or_hash}")))?;
            self.call("getblockhash", &[height.into()])
        } else {
            Ok(height_or_hash.to_owned())
        }
    }

    fn unreachable(&self, reason: String) -> NodeError {
        NodeError::Unreachable {
            url: self.url.clone(),
            reason,
        }
    }
}

impl NodeBackend for CoreRpcBackend {
    fn chain(&self) -> Result<String> {
        let info: serde_json::Value = self.call("getblockchaininfo", &[])?;
        field_str(&info, "chain")
    }

    fn send_raw_transaction(&self, hex: &str) -> Result<String> {
        self.call("sendrawtransaction", &[hex.into()])
    }

    fn raw_transaction(&self, txid: &str, block: Option<&str>) -> Result<String> {
        let mut args: Vec<serde_json::Value> = vec![txid.into(), false.into()];
        if let Some(block) = block {
            args.push(self.block_hash(block)?.into());
        }
        self.call("getrawtransaction", &args)
    }

    fn transaction(&self, txid: &str, block: Option<&str>) -> Result<FetchedTx> {
        let mut args: Vec<serde_json::Value> = vec![txid.into(), 2.into()];
        if let Some(block) = block {
            args.push(self.block_hash(block)?.into());
        }
        let tx: serde_json::Value = self.call("getrawtransaction", &args)?;
        let hex = field_str(&tx, "hex")?;

        let prevouts = tx["vin"].as_array().and_then(|vin| {
            vin.iter()
                .map(|input| {
                    let prevout = input.get("prevout")?;
                    let btc = prevout["value"].as_f64()?;
                    let script = prevout["scriptPubKey"]["hex"].as_str()?.to_owned();
                    Some(((btc * 100_000_000.0).round() as u64, script))
                })
                .collect::<Option<Vec<_>>>()
        });
        Ok(FetchedTx { hex, prevouts })
    }

    fn recent_fee_rates(&self, blocks: usize) -> Result<Vec<(f64, u64)>> {
        let blocks = blocks.max(1);
        let tip: u64 = self.call("getblockcount", &[])?;
        if let Ok(guard) = self.fee_cache.lock() {
            if let Some(cache) = guard.as_ref() {
                if cache.blocks == blocks
                    && cache.tip == tip
                    && cache.taken.elapsed() < FEE_CACHE_TTL
                {
                    return Ok(cache.samples.clone());
                }
            }
        }

        let mut samples = Vec::new();
        for height in tip.saturating_sub(blocks as u64 - 1)..=tip {
            let hash: String = self.call("getblockhash", &[height.into()])?;
            // These are large downloads: one dropped connection should not fail the whole estimate.
            let args = [hash.into(), 2.into()];
            let block: serde_json::Value = match self.call("getblock", &args) {
                Err(NodeError::Unreachable { .. }) => self.call("getblock", &args)?,
                other => other?,
            };
            let Some(txs) = block["tx"].as_array() else {
                continue;
            };
            // The first transaction is the coinbase: it has no fee.
            for tx in txs.iter().skip(1) {
                if let (Some(fee), Some(vsize)) = (tx["fee"].as_f64(), tx["vsize"].as_u64()) {
                    if vsize > 0 {
                        samples.push((fee * 100_000_000.0 / vsize as f64, vsize));
                    }
                }
            }
        }
        if let Ok(mut guard) = self.fee_cache.lock() {
            *guard = Some(FeeCache {
                taken: std::time::Instant::now(),
                tip,
                blocks,
                samples: samples.clone(),
            });
        }
        Ok(samples)
    }

    fn mempool_floor_sat_vb(&self) -> Result<f64> {
        let info: serde_json::Value = self.call("getmempoolinfo", &[])?;
        let per_kvb = |key: &str| info[key].as_f64().unwrap_or(0.0);
        let floor = per_kvb("mempoolminfee").max(per_kvb("minrelaytxfee"));
        if floor <= 0.0 {
            return Err(NodeError::Rpc(
                "node reported no mempool minimum fee".into(),
            ));
        }
        Ok(floor * 100_000.0) // BTC/kvB -> sat/vB
    }

    fn node_status(&self) -> Result<NodeStatus> {
        let info: serde_json::Value = self.call("getblockchaininfo", &[])?;
        let network: serde_json::Value = self.call("getnetworkinfo", &[])?;
        let connections = network["connections"].as_u64().unwrap_or(0) as u32;
        Ok(NodeStatus {
            chain: field_str(&info, "chain")?,
            blocks: field_u64(&info, "blocks")? as u32,
            headers: field_u64(&info, "headers")? as u32,
            sync_percentage: info["verificationprogress"].as_f64().unwrap_or(0.0) * 100.0,
            connections,
        })
    }

    fn block_info(&self, height_or_hash: &str) -> Result<BlockInfo> {
        let hash = if height_or_hash.chars().all(|c| c.is_ascii_digit()) {
            let height: u64 = height_or_hash
                .parse()
                .map_err(|_| NodeError::Rpc(format!("invalid block height {height_or_hash}")))?;
            self.call::<String>("getblockhash", &[height.into()])?
        } else {
            height_or_hash.to_owned()
        };
        let block: serde_json::Value = self.call("getblock", &[hash.into(), 1.into()])?;
        Ok(BlockInfo {
            hash: field_str(&block, "hash")?,
            height: field_u64(&block, "height")? as u32,
            time: field_u64(&block, "time")?,
            tx_count: field_u64(&block, "nTx")? as usize,
            size: field_u64(&block, "size")? as usize,
            version: field_u64(&block, "version")? as u32,
            previous_block_hash: block["previousblockhash"].as_str().map(str::to_owned),
            merkle_root: field_str(&block, "merkleroot")?,
            bits: field_str(&block, "bits")?,
            difficulty: block["difficulty"].as_f64().unwrap_or(0.0),
        })
    }

    fn fee_estimate(&self, target: u16, mode: &str) -> Result<FeeEstimate> {
        let est: serde_json::Value = self.call(
            "estimatesmartfee",
            &[target.into(), mode.to_uppercase().into()],
        )?;
        let btc_per_kvb = est["feerate"].as_f64().ok_or_else(|| {
            NodeError::Rpc(
                "node returned no fee estimate (a fresh node needs more blocks/mempool data); \
                 pass --fallback-rate to use a fixed rate"
                    .into(),
            )
        })?;
        Ok(FeeEstimate {
            sat_vb: btc_per_kvb * 100_000.0,
            mode: mode.to_owned(),
            target_blocks: target,
            is_fallback: false,
        })
    }
}

fn field_str(v: &serde_json::Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| NodeError::Rpc(format!("node response is missing `{key}`")))
}

fn field_u64(v: &serde_json::Value, key: &str) -> Result<u64> {
    v[key]
        .as_u64()
        .ok_or_else(|| NodeError::Rpc(format!("node response is missing `{key}`")))
}

/// Mock implementation for testing (no bitcoind required).
pub struct MockBackend;

impl NodeBackend for MockBackend {
    fn send_raw_transaction(&self, _hex: &str) -> Result<String> {
        Ok("0".repeat(64))
    }

    fn node_status(&self) -> Result<NodeStatus> {
        Ok(NodeStatus {
            chain: "regtest".into(),
            blocks: 101,
            headers: 101,
            sync_percentage: 100.0,
            connections: 1,
        })
    }

    fn block_info(&self, _height_or_hash: &str) -> Result<BlockInfo> {
        Ok(BlockInfo {
            hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            height: 1,
            time: 1296688602,
            tx_count: 1,
            size: 285,
            version: 1,
            previous_block_hash: None,
            merkle_root: "4a5e1e".into(),
            bits: "207fffff".into(),
            difficulty: 4.6565423730713754e-10,
        })
    }

    fn fee_estimate(&self, target: u16, mode: &str) -> Result<FeeEstimate> {
        Ok(FeeEstimate {
            sat_vb: 1.0,
            mode: mode.to_string(),
            target_blocks: target,
            is_fallback: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_backend_returns_fixed_data() {
        let backend = MockBackend;
        let status = backend.node_status().unwrap();
        assert_eq!(status.chain, "regtest");
        assert_eq!(status.blocks, 101);
        assert_eq!(status.sync_percentage, 100.0);
    }

    #[test]
    fn mock_backend_fee_estimate_is_fallback() {
        let backend = MockBackend;
        let fee = backend.fee_estimate(6, "economical").unwrap();
        assert!(fee.is_fallback);
        assert_eq!(fee.sat_vb, 1.0);
    }
}
