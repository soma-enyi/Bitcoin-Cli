use btc_core::CoreError;
use btc_node::NodeError;
use thiserror::Error;

/// Every error a command can end with. Each one maps to a stable `kind` (for JSON
/// output) and an exit code (for scripts).
#[derive(Debug, Error)]
pub enum AppError {
    #[error("`{0}` is not implemented yet")]
    NotImplemented(&'static str),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("invalid input: {0}")]
    Input(String),

    #[error("failed to read input: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Core(#[from] CoreError),

    #[error(transparent)]
    Node(#[from] NodeError),

    #[error("failed to serialize output: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::NotImplemented(_) => "not_implemented",
            AppError::Config(_) => "invalid_config",
            AppError::Core(CoreError::WrongNetwork { .. }) => "wrong_network",
            AppError::Input(_) | AppError::Core(_) => "invalid_input",
            AppError::Io(_) => "io",
            AppError::Node(_) => "node",
            AppError::Serialize(_) => "internal",
        }
    }

    /// 1 = the user or their input is at fault, 2 = the node or network is.
    pub fn exit_code(&self) -> u8 {
        match self {
            AppError::Node(_) => 2,
            AppError::NotImplemented(_)
            | AppError::Config(_)
            | AppError::Input(_)
            | AppError::Io(_)
            | AppError::Core(_)
            | AppError::Serialize(_) => 1,
        }
    }
}
