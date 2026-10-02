use clap::Subcommand;
use clap::builder::PossibleValuesParser;
use clap::builder::TypedValueParser;

#[derive(Subcommand)]
pub enum MnemonicCmd {
    New {
        #[arg(
            long,
            default_value = "12",
            value_parser = PossibleValuesParser::new(["12", "15", "18", "21", "24"])
                .map(|s| s.parse::<usize>().expect("possible values are numbers"))
        )]
        words: usize,

        #[arg(long)]
        passphrase: Option<String>,
    },

    ToXprv {
        mnemonic: String,

        #[arg(long)]
        passphrase: Option<String>,
    },
}
