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

/// A fee estimate taken from what recent blocks paid, for `fee estimate --block/--blocks`.
/// `end` is the last block of the window (the tip when `None`) and `blocks` how many to read
/// (a default for the target when `None`). The estimate is the median of each block's own.
pub fn fee_estimate_from_blocks(
    ctx: &Context,
    end: Option<u32>,
    blocks: Option<u16>,
    target: Option<u16>,
) -> Result<FeeEstimateOutput, AppError> {
    let target = target.unwrap_or(6);
    validate::fee_target(target)?;
    let blocks = blocks.unwrap_or_else(|| btc_node::fee_source::default_window(target));
    if !(1..=1008).contains(&blocks) {
        return Err(AppError::Input(format!(
            "the number of blocks must be 1-1008, got {blocks}"
        )));
    }

    ensure_chain(ctx)?;
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    let tip = backend.node_status()?.blocks;
    let end = end.unwrap_or(tip);
    if end > tip {
        return Err(AppError::Input(format!(
            "block {end} is above the node's tip ({tip})"
        )));
    }
    let first = end.saturating_sub(u32::from(blocks) - 1);

    let mut rates = Vec::new();
    let mut lowest = f64::INFINITY;
    let mut highest = 0.0_f64;
    let mut skipped = 0;
    for height in first..=end {
        match backend.block_fee_stats(&height.to_string()) {
            Ok(stats) => {
                let rate = btc_node::fee_source::estimate_from_block_stats(&stats, target);
                lowest = lowest.min(rate);
                highest = highest.max(rate);
                rates.push(rate);
            }
            // A node that is down fails every block: report it once, not as skipped blocks.
            Err(e @ btc_node::NodeError::Unreachable { .. }) => return Err(e.into()),
            // Pruned, or only a coinbase: the other blocks still count.
            Err(_) => skipped += 1,
        }
    }
    let used = rates.len();
    let Some(rate) = btc_node::fee_source::median(&mut rates) else {
        return Err(AppError::Input(format!(
            "no fee data in blocks {first}-{end} (a pruned node, or blocks with only a coinbase)"
        )));
    };

    let skipped_note = if skipped > 0 {
        format!(" {skipped} block(s) had no fee data and were skipped.")
    } else {
        String::new()
    };
    Ok(FeeEstimateOutput {
        sat_vb: rate,
        // The node's economical/conservative modes do not apply to a block window.
        mode: "from blocks".to_owned(),
        target_blocks: target,
        is_fallback: false,
        source: format!("blocks {first}-{end} ({used} read through your node)"),
        note: Some(format!(
            "The median of what each of these blocks paid (per-block range {lowest:.2}-{highest:.2} \
             sat/vB).{skipped_note} Past blocks show what confirmed then, not what will confirm next."
        )),
    })
}

/// Mines blocks to a wallet's address on regtest, so there is something to spend.
///
/// A wallet with no spendable coin gets 101 blocks (the first reward then matures at once);
/// one that has coins gets a single block, which confirms waiting payments.
pub fn mine_for_wallet(ctx: &Context, wallet: &super::wallets::Wallet) -> Result<String, AppError> {
    if ctx.network != btc_core::Network::Regtest {
        return Err(AppError::Input(
            "mining only works on regtest: other networks need real proof of work".into(),
        ));
    }
    ensure_chain(ctx)?;
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    let tip = backend.node_status()?.blocks;
    let has_coins = backend
        .list_utxos(&wallet.address)?
        .iter()
        .any(|u| super::send::is_spendable(u, tip));
    let blocks = if has_coins { 1 } else { 101 };
    backend.generate_to_address(blocks, &wallet.address)?;
    let tip = backend.node_status()?.blocks;
    Ok(if blocks == 1 {
        format!(
            "Mined 1 block to `{}`. Tip is {tip}. Waiting payments are confirmed.",
            wallet.name
        )
    } else {
        format!(
            "Mined {blocks} blocks to `{}`. Tip is {tip}. The first reward is spendable now.",
            wallet.name
        )
    })
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
    fn mining_gives_a_new_wallet_101_blocks_and_refuses_other_networks() {
        let wallet = super::super::wallets::Wallet {
            name: "alice".into(),
            address: "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".into(),
        };
        // The mock wallet already holds coins, so one block is enough.
        let mut c = ctx(None);
        c.backend = Some(Arc::new(btc_node::MockBackend));
        let text = mine_for_wallet(&c, &wallet).unwrap();
        assert!(text.contains("Mined 1 block"), "{text}");

        c.network = Network::Signet;
        let err = mine_for_wallet(&c, &wallet).unwrap_err().to_string();
        assert!(err.contains("only works on regtest"), "{err}");
    }

    /// A regtest node whose wallet has no coins, counting the blocks it is asked to mine.
    struct EmptyChain(std::sync::atomic::AtomicU32);

    impl NodeBackend for EmptyChain {
        fn node_status(&self) -> btc_node::backend::Result<NodeStatus> {
            btc_node::MockBackend.node_status()
        }
        fn block_info(&self, h: &str) -> btc_node::backend::Result<BlockInfo> {
            btc_node::MockBackend.block_info(h)
        }
        fn fee_estimate(&self, t: u16, m: &str) -> btc_node::backend::Result<FeeEstimate> {
            btc_node::MockBackend.fee_estimate(t, m)
        }
        fn list_utxos(&self, _: &str) -> btc_node::backend::Result<Vec<btc_node::Utxo>> {
            Ok(Vec::new())
        }
        fn generate_to_address(
            &self,
            blocks: u32,
            _: &str,
        ) -> btc_node::backend::Result<Vec<String>> {
            self.0.store(blocks, std::sync::atomic::Ordering::SeqCst);
            Ok(Vec::new())
        }
        fn send_raw_transaction(&self, _: &str) -> btc_node::backend::Result<String> {
            Err(NodeError::Rpc("unused".into()))
        }
    }

    #[test]
    fn a_wallet_without_coins_gets_101_blocks_so_the_first_reward_is_spendable() {
        let wallet = super::super::wallets::Wallet {
            name: "alice".into(),
            address: "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".into(),
        };
        let chain = Arc::new(EmptyChain(std::sync::atomic::AtomicU32::new(0)));
        let mut c = ctx(None);
        c.backend = Some(chain.clone());
        let text = mine_for_wallet(&c, &wallet).unwrap();
        assert_eq!(chain.0.load(std::sync::atomic::Ordering::SeqCst), 101);
        assert!(text.contains("Mined 101 blocks"), "{text}");
    }

    #[test]
    fn a_block_window_estimates_from_those_blocks_fees() {
        let mut c = ctx(None);
        c.backend = Some(Arc::new(btc_node::MockBackend));
        let out = fee_estimate_from_blocks(&c, Some(1), Some(1), Some(6)).unwrap();
        // Mock block: 10th percentile 2.0 for a 6-block target.
        assert_eq!(out.sat_vb, 2.0);
        assert!(
            out.source.contains("blocks 0-1") || out.source.contains("1-1"),
            "{}",
            out.source
        );
        // The mock node's tip is 101; a window ending beyond it is refused.
        assert!(fee_estimate_from_blocks(&c, Some(500), None, Some(6)).is_err());
        assert!(fee_estimate_from_blocks(&c, None, Some(0), Some(6)).is_err());
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
