use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum PsbtCmd {
    /// Show inputs, outputs, signature counts and fee of a base64 PSBT
    Analyze {
        /// Base64 PSBT, or `-` to read from stdin
        psbt: String,
    },

    /// Merge PSBTs that describe the same transaction (e.g. one per signer)
    Combine {
        /// Two or more base64 PSBTs
        #[arg(required = true, num_args = 2..)]
        psbts: Vec<String>,
    },
}
