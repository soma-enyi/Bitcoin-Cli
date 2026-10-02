use std::sync::Arc;

use btc_core::Network;
use btc_node::{NodeBackend, RpcConfig};

use crate::output::OutputMode;

/// Everything a command needs besides its own arguments, resolved once at startup.
/// Where a mainnet fee estimate comes from when the node cannot give one. Test networks
/// always use the node's own estimator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FeeSource {
    /// Only the node's estimator (then the labelled mempool minimum).
    Node,
    /// A public Esplora-style `fee-estimates` service.
    Public,
    /// Estimated from the fee rates in the last few blocks, read through the node.
    Blocks,
}

#[derive(Clone, Debug)]
pub struct FeeConfig {
    pub source: FeeSource,
    pub api_url: String,
    pub blocks: usize,
}

impl Default for FeeConfig {
    fn default() -> Self {
        FeeConfig {
            source: FeeSource::Node,
            api_url: btc_node::fee_source::DEFAULT_PUBLIC_URL.to_owned(),
            blocks: 3,
        }
    }
}

pub struct Context {
    pub network: Network,
    pub output: OutputMode,
    pub rpc: RpcConfig,
    pub backend: Option<Arc<dyn NodeBackend>>,
    pub fees: FeeConfig,
}
