use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::CoreError;

/// The networks this tool operates on.
///
/// This is our own type rather than `bitcoin::Network` so the set of supported chains is
/// explicit. `Mainnet` handles real funds: `is_mainnet` lets callers add extra warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Regtest,
    Signet,
    Testnet4,
}

impl Network {
    pub const ALL: [Network; 4] = [
        Network::Mainnet,
        Network::Regtest,
        Network::Signet,
        Network::Testnet4,
    ];

    pub fn is_mainnet(self) -> bool {
        self == Network::Mainnet
    }

    /// The name Bitcoin Core uses for this chain (`-chain=<name>`, data subdirectory).
    pub fn as_str(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Regtest => "regtest",
            Network::Signet => "signet",
            Network::Testnet4 => "testnet4",
        }
    }
}

impl From<Network> for bitcoin::Network {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => bitcoin::Network::Bitcoin,
            Network::Regtest => bitcoin::Network::Regtest,
            Network::Signet => bitcoin::Network::Signet,
            Network::Testnet4 => bitcoin::Network::Testnet4,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Network {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Network::ALL
            .into_iter()
            .find(|n| n.as_str() == s)
            .ok_or_else(|| CoreError::InvalidInput(format!("unknown network `{s}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_round_trip() {
        for network in Network::ALL {
            assert_eq!(network.to_string().parse::<Network>().unwrap(), network);
        }
    }

    #[test]
    fn mainnet_parses_by_its_canonical_name_only() {
        assert_eq!("mainnet".parse::<Network>().unwrap(), Network::Mainnet);
        assert!("bitcoin".parse::<Network>().is_err());
        assert!(Network::Mainnet.is_mainnet());
        assert!(!Network::Regtest.is_mainnet());
    }

    #[test]
    fn maps_to_rust_bitcoin_networks() {
        assert_eq!(
            bitcoin::Network::from(Network::Testnet4),
            bitcoin::Network::Testnet4
        );
        assert_eq!(bitcoin::Network::from(Network::Mainnet), bitcoin::Network::Bitcoin);
        // Only mainnet may map to mainnet prefixes.
        for network in Network::ALL {
            assert_eq!(
                bitcoin::NetworkKind::from(bitcoin::Network::from(network)).is_mainnet(),
                network.is_mainnet()
            );
        }
    }
}
