use crate::app::error::AppError;
use crate::app::{Context, FeeSource, validate};
use crate::output::Render;

#[derive(Debug, serde::Serialize)]
pub struct NodeStatusOutput {
    pub chain: String,
    pub blocks: u32,
    pub headers: u32,
    pub sync_percentage: f64,
    pub connections: u32,
}

impl Render for NodeStatusOutput {
    fn render_human(&self) -> String {
        format!(
            "Chain: {}\nBlocks: {}\nHeaders: {}\nSync: {:.1}%\nConnections: {}",
            self.chain, self.blocks, self.headers, self.sync_percentage, self.connections
        )
    }
}

#[derive(Debug, serde::Serialize)]
pub struct BlockInfoOutput {
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

impl Render for BlockInfoOutput {
    fn render_human(&self) -> String {
        format!(
            "Hash: {}\nHeight: {}\nTime: {}\nTransactions: {}\nSize: {} bytes\nVersion: {}\nMerkle Root: {}\nBits: {}\nDifficulty: {:.2e}",
            self.hash,
            self.height,
            self.time,
            self.tx_count,
            self.size,
            self.version,
            self.merkle_root,
            self.bits,
            self.difficulty
        )
    }
}

#[derive(Debug, serde::Serialize)]
pub struct FeeEstimateOutput {
    pub sat_vb: f64,
    pub mode: String,
    pub target_blocks: u16,
    pub is_fallback: bool,
    /// Where the number came from: the node's estimator, a public service, recent blocks, ...
    pub source: String,
    /// Set when `sat_vb` is not the node's own estimate, and what that means for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Render for FeeEstimateOutput {
    fn render_human(&self) -> String {
        let fallback_note = if self.is_fallback { " (fallback)" } else { "" };
        let line = format!(
            "{:.2} sat/vB (mode: {}, target: {} blocks){}\nsource: {}",
            self.sat_vb, self.mode, self.target_blocks, fallback_note, self.source
        );
        match &self.note {
            Some(note) => format!("{line}\n\n{note}"),
            None => line,
        }
    }
}

/// Bitcoin Core's name for the chain of each network.
fn expected_chain(network: btc_core::Network) -> &'static str {
    match network {
        btc_core::Network::Mainnet => "main",
        other => other.as_str(),
    }
}

/// Refuses to continue when the node is on a different chain than the selected network,
/// e.g. `BTC_NETWORK=regtest` while the node settings still point at a mainnet gateway.
pub fn ensure_chain(ctx: &Context) -> Result<(), AppError> {
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    check_chain(ctx, &backend.chain()?)
}

fn check_chain(ctx: &Context, chain: &str) -> Result<(), AppError> {
    let expected = expected_chain(ctx.network);
    if chain != expected {
        let upper = ctx.network.as_str().to_uppercase();
        return Err(AppError::Config(format!(
            "the node is on chain `{chain}` but the selected network is {} (`{expected}`). \
             Set the node for this network with BTC_{upper}_RPC_URL (and BTC_{upper}_RPC_COOKIE \
             or BTC_{upper}_RPC_API_KEY), or pick the matching network with BTC_NETWORK / -n",
            ctx.network
        )));
    }
    Ok(())
}

pub fn status(ctx: &Context) -> Result<NodeStatusOutput, AppError> {
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    let status = backend.node_status()?;
    check_chain(ctx, &status.chain)?;

    Ok(NodeStatusOutput {
        chain: status.chain,
        blocks: status.blocks,
        headers: status.headers,
        sync_percentage: status.sync_percentage,
        connections: status.connections,
    })
}

pub fn block_info(ctx: &Context, height_or_hash: &str) -> Result<BlockInfoOutput, AppError> {
    if height_or_hash.chars().all(|c| c.is_ascii_digit()) {
        validate::block_height(height_or_hash)?;
    } else {
        validate::block_hash(height_or_hash)?;
    }

    ensure_chain(ctx)?;
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    let block = backend.block_info(height_or_hash)?;

    Ok(BlockInfoOutput {
        hash: block.hash,
        height: block.height,
        time: block.time,
        tx_count: block.tx_count,
        size: block.size,
        version: block.version,
        previous_block_hash: block.previous_block_hash,
        merkle_root: block.merkle_root,
        bits: block.bits,
        difficulty: block.difficulty,
    })
}

fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
}

pub fn fee_estimate(
    ctx: &Context,
    target: Option<u16>,
    mode: Option<&str>,
    fallback_rate: Option<f64>,
) -> Result<FeeEstimateOutput, AppError> {
    let target = target.unwrap_or(6);
    validate::fee_target(target)?;
    let mode = mode.unwrap_or("economical");

    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;

    // A wrong-chain node is always an error; an unreachable one may fall back to --fallback-rate.
    match ensure_chain(ctx) {
        Err(AppError::Node(_)) if fallback_rate.is_some() => {}
        Err(e) => return Err(e),
        Ok(()) => {}
    }

    let output =
        |sat_vb: f64, is_fallback: bool, source: String, note: Option<String>| FeeEstimateOutput {
            sat_vb,
            mode: mode.to_string(),
            target_blocks: target,
            is_fallback,
            source,
            note,
        };

    let node_error = match backend.fee_estimate(target, mode) {
        Ok(fee) => {
            return Ok(FeeEstimateOutput {
                sat_vb: fee.sat_vb,
                mode: fee.mode,
                target_blocks: fee.target_blocks,
                is_fallback: false,
                source: "the node's estimator".to_owned(),
                note: None,
            });
        }
        Err(e) => e,
    };

    if let Some(rate) = fallback_rate {
        return Ok(output(rate, true, "your --fallback-rate".to_owned(), None));
    }

    // Only mainnet has other sources: test networks use the node's own estimator and nothing else.
    let mut alternative_failed = None;
    if ctx.network.is_mainnet() {
        match ctx.fees.source {
            FeeSource::Public => {
                match btc_node::fee_source::fetch_public(&ctx.fees.api_url, target) {
                    Ok(rate) => {
                        return Ok(output(
                        rate,
                        false,
                        format!("public fee service ({})", host_of(&ctx.fees.api_url)),
                        Some(
                            "An estimate from a public service, not from your node. The service \
                             sees your IP address. Fee rates change quickly: check again before sending."
                                .to_owned(),
                        ),
                    ));
                    }
                    Err(e) => {
                        alternative_failed = Some(format!("the public fee service failed: {e}"))
                    }
                }
            }
            FeeSource::Blocks => {
                let estimate = backend
                    .recent_fee_rates(ctx.fees.blocks)
                    .map_err(AppError::Node)
                    .and_then(|samples| {
                        btc_node::fee_source::estimate_from_samples(&samples, target)
                            .map_err(AppError::Node)
                    });
                match estimate {
                    Ok(rate) => {
                        return Ok(output(
                            rate,
                            false,
                            format!("the last {} blocks, read through your node", ctx.fees.blocks),
                            Some(
                                "An approximation from what recently confirmed, not the node's own \
                                 estimator. When blocks are not full this is close to the 1 sat/vB \
                                 minimum. Check again before sending."
                                    .to_owned(),
                            ),
                        ));
                    }
                    Err(e) => {
                        alternative_failed =
                            Some(format!("estimating from recent blocks failed: {e}"))
                    }
                }
            }
            FeeSource::Node => {}
        }
    }

    let Ok(floor) = backend.mempool_floor_sat_vb() else {
        return Err(AppError::Node(node_error));
    };
    let mut note = if ctx.network.is_mainnet() {
        "This is only the mempool MINIMUM, not a confirmation estimate: this node does not provide \
         fee estimates. Real confirmation usually needs more. Set BTC_FEE_SOURCE=public (or blocks) \
         for an estimate, or pass your own rate to `tx create --fee-rate`."
            .to_owned()
    } else {
        "This is only the mempool MINIMUM, not an estimate: the node has no fee data yet. On regtest, \
         mine blocks that contain transactions, plus a number of blocks after them, so it can learn \
         fee rates. Or pass your own rate to `tx create --fee-rate`."
            .to_owned()
    };
    if let Some(why) = alternative_failed {
        note.push_str(&format!("\n\nNote: {why}."));
    }
    Ok(output(
        floor,
        false,
        "the mempool minimum".to_owned(),
        Some(note),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use btc_core::Network;
    use btc_node::{
        BlockInfo, FeeEstimate, NodeBackend, NodeError, NodeStatus, RpcConfig, RpcOptions,
    };

    use super::*;
    use crate::output::{OutputMode, Render};

    /// A node that is on regtest but cannot estimate fees, like a restricted gateway.
    struct NoEstimates {
        floor: Option<f64>,
    }

    impl NodeBackend for NoEstimates {
        fn node_status(&self) -> btc_node::backend::Result<NodeStatus> {
            Ok(NodeStatus {
                chain: "regtest".into(),
                blocks: 1,
                headers: 1,
                sync_percentage: 100.0,
                connections: 1,
            })
        }
        fn block_info(&self, _: &str) -> btc_node::backend::Result<BlockInfo> {
            Err(NodeError::Rpc("unused".into()))
        }
        fn fee_estimate(&self, _: u16, _: &str) -> btc_node::backend::Result<FeeEstimate> {
            Err(NodeError::Rpc("estimatesmartfee is not permitted".into()))
        }
        fn mempool_floor_sat_vb(&self) -> btc_node::backend::Result<f64> {
            self.floor
                .ok_or_else(|| NodeError::Rpc("no mempool".into()))
        }
        fn send_raw_transaction(&self, _: &str) -> btc_node::backend::Result<String> {
            Err(NodeError::Rpc("unused".into()))
        }
    }

    fn ctx(floor: Option<f64>) -> Context {
        Context {
            network: Network::Regtest,
            output: OutputMode::Human,
            rpc: RpcConfig::resolve(Network::Regtest, RpcOptions::default()),
            backend: Some(Arc::new(NoEstimates { floor })),
            fees: crate::app::FeeConfig::default(),
        }
    }

    #[test]
    fn without_estimates_the_mempool_floor_is_shown_and_labelled() {
        let out = fee_estimate(&ctx(Some(1.0)), Some(6), None, None).unwrap();
        assert_eq!(out.sat_vb, 1.0);
        assert!(!out.is_fallback);
        let text = out.render_human();
        assert!(
            text.contains("MINIMUM") && text.contains("--fee-rate"),
            "{text}"
        );
    }

    #[test]
    fn an_explicit_fallback_rate_beats_the_floor() {
        let out = fee_estimate(&ctx(Some(1.0)), Some(6), None, Some(7.0)).unwrap();
        assert_eq!(out.sat_vb, 7.0);
        assert!(out.is_fallback && out.note.is_none());
    }

    #[test]
    fn with_neither_estimate_nor_floor_it_is_an_error() {
        assert!(fee_estimate(&ctx(None), Some(6), None, None).is_err());
    }
}
