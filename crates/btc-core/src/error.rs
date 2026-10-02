use thiserror::Error;

/// Errors produced by offline Bitcoin logic.
///
/// Variants are added as each feature lands; `#[non_exhaustive]` lets us do that
/// without breaking code that matches on this enum.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("invalid mnemonic: {0}")]
    Mnemonic(#[from] bip39::Error),

    #[error("key derivation failed: {0}")]
    Bip32(#[from] bitcoin::bip32::Error),

    #[error("invalid address: {0}")]
    InvalidAddress(String),

    #[error("{address} is a {actual} address, not valid on {selected}")]
    WrongNetwork {
        address: String,
        actual: String,
        selected: String,
    },
}
