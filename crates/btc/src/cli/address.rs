use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum AddressCmd {
    FromPubkey {
        pubkey: String,

        #[arg(long = "type", value_enum, default_value_t = AddressTypeArg::All)]
        addr_type: AddressTypeArg,
    },

    Validate {
        address: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum AddressTypeArg {
    P2pkh,
    P2shP2wpkh,
    P2wpkh,
    P2tr,
    All,
}

impl AddressTypeArg {
    pub fn selected(self) -> Option<btc_core::address::AddressType> {
        use btc_core::address::AddressType;
        match self {
            AddressTypeArg::P2pkh => Some(AddressType::P2pkh),
            AddressTypeArg::P2shP2wpkh => Some(AddressType::P2shP2wpkh),
            AddressTypeArg::P2wpkh => Some(AddressType::P2wpkh),
            AddressTypeArg::P2tr => Some(AddressType::P2tr),
            AddressTypeArg::All => None,
        }
    }
}
