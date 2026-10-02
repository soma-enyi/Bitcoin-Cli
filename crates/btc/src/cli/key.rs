use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum KeyCmd {
    Generate {
        #[arg(long = "type", value_enum, default_value_t = KeyTypeArg::Ecdsa)]
        key_type: KeyTypeArg,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum KeyTypeArg {
    Ecdsa,
    Schnorr,
}

impl From<KeyTypeArg> for btc_core::keys::KeyType {
    fn from(arg: KeyTypeArg) -> Self {
        match arg {
            KeyTypeArg::Ecdsa => btc_core::keys::KeyType::Ecdsa,
            KeyTypeArg::Schnorr => btc_core::keys::KeyType::Schnorr,
        }
    }
}
