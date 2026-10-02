use clap::{Args, ValueEnum};

#[derive(Args)]
pub struct DeriveArgs {
    pub key: String,

    pub path: String,

    #[arg(long, default_value_t = 1)]
    pub count: u32,

    #[arg(long = "addr-type", value_enum)]
    pub addr_type: Option<DeriveAddrType>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum DeriveAddrType {
    P2pkh,
    P2shP2wpkh,
    P2wpkh,
    P2tr,
}

impl From<DeriveAddrType> for btc_core::address::AddressType {
    fn from(arg: DeriveAddrType) -> Self {
        use btc_core::address::AddressType;
        match arg {
            DeriveAddrType::P2pkh => AddressType::P2pkh,
            DeriveAddrType::P2shP2wpkh => AddressType::P2shP2wpkh,
            DeriveAddrType::P2wpkh => AddressType::P2wpkh,
            DeriveAddrType::P2tr => AddressType::P2tr,
        }
    }
}
