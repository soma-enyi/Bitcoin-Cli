use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum MultisigCmd {
    /// Build an m-of-n script and its P2SH / P2WSH / P2SH-P2WSH addresses
    Create {
        /// Signatures required (m)
        #[arg(long)]
        threshold: usize,

        /// Compressed public keys in hex (n of them)
        #[arg(long = "pubkey", required = true, num_args = 1..)]
        pubkeys: Vec<String>,

        /// Keep the given key order instead of sorting (BIP67)
        #[arg(long)]
        keep_order: bool,
    },

    /// Read an m-of-n script (hex) back into its threshold and keys
    Analyze {
        /// Script hex, or `-` to read from stdin
        script: String,
    },
}
