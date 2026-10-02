//! Bitcoin Core connectivity.
//!
//! Online features go through the `NodeBackend` trait so the RPC client can be swapped
//! and tests can run against a mock without a running bitcoind.

pub mod backend;
pub mod config;
pub mod error;
pub mod fee_source;

pub use backend::{
    BlockInfo, CoreRpcBackend, FeeEstimate, FetchedTx, MockBackend, NodeBackend, NodeStatus,
};
pub use config::{RpcAuth, RpcConfig, RpcOptions};
pub use error::NodeError;
