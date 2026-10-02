//! Use cases: one function per feature. Services combine `btc_core` (offline logic)
//! and `btc_node` (the node) and return plain data. They never print; `output`
//! decides how results are shown, so the CLI and the TUI can share them.

pub mod address;
pub mod config;
pub mod context;
pub mod derive;
pub mod error;
pub mod input;
pub mod keys;
pub mod mnemonic;
pub mod multisig;
pub mod node;
pub mod psbt;
pub mod tx;
pub mod validate;

pub use context::{Context, FeeConfig, FeeSource};
pub use error::AppError;
