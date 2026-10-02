use std::path::PathBuf;

use clap::{ArgGroup, Subcommand};

#[derive(Subcommand)]
pub enum TxCmd {
    Decode {
        hex: String,

        #[arg(long)]
        prevouts_from_node: bool,
    },

    Create {
        #[arg(long = "input", required = true, value_name = "TXID:VOUT:SATS:ADDRESS")]
        inputs: Vec<String>,

        #[arg(long = "to", required = true, value_name = "ADDRESS:SATS")]
        outputs: Vec<String>,

        #[arg(long)]
        change: Option<String>,

        #[arg(long, value_name = "SAT/VB")]
        fee_rate: f64,
    },

    #[command(group(ArgGroup::new("key_source").required(true).args(["key", "key_file"])))]
    Sign {
        psbt: String,

        #[arg(long)]
        key: Option<String>,

        #[arg(long)]
        key_file: Option<PathBuf>,
    },

    /// Send a signed transaction to the node
    Broadcast {
        /// Signed transaction hex, or `-` to read from stdin
        hex: String,

        /// Required on mainnet: broadcasting spends real bitcoin and cannot be undone
        #[arg(long)]
        yes: bool,
    },
}
