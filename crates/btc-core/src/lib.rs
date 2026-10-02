//! Offline Bitcoin logic: keys, mnemonics, derivation, addresses and transactions.
//!
//! This crate performs no network or terminal I/O. Every function returns plain data
//! (`Serialize` structs) and leaves presentation to the front ends.

pub mod address;
pub mod derive;
pub mod error;
pub mod keys;
pub mod mnemonic;
pub mod multisig;
pub mod network;
pub mod script;
pub mod tx;

pub use error::CoreError;
pub use network::Network;
