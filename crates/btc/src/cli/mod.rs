//! Command-line definitions only. No logic lives here: clap parses `argv` into
//! these types and `dispatch` hands them to the services in `app`.

mod address;
mod block;
mod derive;
mod fee;
mod key;
mod mnemonic;
pub mod multisig;
mod node;
pub mod psbt;
mod tx;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

pub use address::AddressCmd;
pub use block::BlockCmd;
pub use derive::DeriveArgs;
pub use fee::FeeCmd;
pub use fee::FeeMode;
pub use key::KeyCmd;
pub use mnemonic::MnemonicCmd;
pub use multisig::MultisigCmd;
pub use node::NodeCmd;
pub use psbt::PsbtCmd;
pub use tx::TxCmd;

#[derive(Parser)]
#[command(name = "btc", version, propagate_version = true)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args)]
pub struct GlobalArgs {
    #[arg(
        long,
        short = 'n',
        global = true,
        value_enum,
        env = "BTC_NETWORK",
        default_value_t = NetworkArg::Regtest
    )]
    pub network: NetworkArg,

    #[arg(long, global = true)]
    pub json: bool,

    #[command(flatten)]
    pub rpc: RpcArgs,

    #[command(flatten)]
    pub fees: FeeArgs,
}

#[derive(Args)]
#[command(next_help_heading = "Fee estimates (mainnet)")]
pub struct FeeArgs {
    /// Where mainnet fee estimates come from when the node cannot estimate. Test networks
    /// always use the node's own estimator
    #[arg(long, global = true, value_enum, env = "BTC_FEE_SOURCE", default_value_t = FeeSourceArg::Node)]
    pub fee_source: FeeSourceArg,

    /// Esplora-compatible fee-estimates URL for --fee-source public (https only)
    #[arg(long, global = true, env = "BTC_FEE_API_URL")]
    pub fee_api_url: Option<String>,

    /// How many recent blocks --fee-source blocks reads (about 10 MB each)
    #[arg(long, global = true, env = "BTC_FEE_BLOCKS", default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=10))]
    pub fee_blocks: u8,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FeeSourceArg {
    /// Only the node's own estimator, then the labelled mempool minimum
    Node,
    /// A public fee service (the service learns your IP address)
    Public,
    /// Computed from recent blocks read through your node: private but heavy
    Blocks,
}

#[derive(Args)]
#[command(next_help_heading = "Node connection")]
pub struct RpcArgs {
    #[arg(long, global = true, env = "BTC_RPC_URL")]
    pub rpc_url: Option<String>,

    #[arg(long, global = true, env = "BTC_RPC_COOKIE")]
    pub rpc_cookie: Option<PathBuf>,

    #[arg(long, global = true, env = "BTC_RPC_USER")]
    pub rpc_user: Option<String>,

    #[arg(long, global = true, env = "BTC_RPC_PASSWORD", hide_env_values = true)]
    pub rpc_password: Option<String>,

    /// API key for a hosted node gateway (sent as `X-API-Key`); needs --rpc-url
    #[arg(long, global = true, env = "BTC_RPC_API_KEY", hide_env_values = true)]
    pub rpc_api_key: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum NetworkArg {
    Regtest,
    Signet,
    Testnet4,
    /// Real funds. Offline features work here too: keep secrets safe.
    Mainnet,
}

impl From<NetworkArg> for btc_core::Network {
    fn from(arg: NetworkArg) -> Self {
        match arg {
            NetworkArg::Regtest => btc_core::Network::Regtest,
            NetworkArg::Signet => btc_core::Network::Signet,
            NetworkArg::Testnet4 => btc_core::Network::Testnet4,
            NetworkArg::Mainnet => btc_core::Network::Mainnet,
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    #[command(subcommand)]
    Key(KeyCmd),

    #[command(subcommand)]
    Mnemonic(MnemonicCmd),

    Derive(DeriveArgs),

    #[command(subcommand)]
    Address(AddressCmd),

    #[command(subcommand)]
    Tx(TxCmd),

    #[command(subcommand)]
    Block(BlockCmd),

    #[command(subcommand)]
    Fee(FeeCmd),

    #[command(subcommand)]
    Node(NodeCmd),

    #[command(subcommand)]
    Psbt(PsbtCmd),

    #[command(subcommand)]
    Multisig(MultisigCmd),

    Config,

    Tui,

    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}
