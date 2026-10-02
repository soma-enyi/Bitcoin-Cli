use thiserror::Error;

/// Errors from talking to a Bitcoin Core node.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NodeError {
    #[error("could not reach node at {url}: {reason}")]
    Unreachable { url: String, reason: String },

    #[error("node returned an error: {0}")]
    Rpc(String),
}
