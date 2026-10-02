//! Private/public key pair generation.

use bitcoin::PrivateKey;
use bitcoin::secp256k1::{Keypair, Parity, Secp256k1, SecretKey, rand};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::Network;

/// Which signature scheme a key is meant for. The secret key is the same kind of
/// number either way; only the public key encoding differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    /// Legacy and SegWit v0 (P2PKH, P2WPKH): 33-byte compressed public key.
    Ecdsa,
    /// Taproot (P2TR, BIP340): 32-byte x-only public key.
    Schnorr,
}

/// A key pair. Secret fields are wiped from memory when this is dropped.
#[derive(Serialize)]
pub struct KeyPairInfo {
    pub network: Network,
    pub key_type: KeyType,
    /// Wallet Import Format: base58check(version byte + key + 0x01 compressed flag).
    /// Every test network uses version 0xEF, so these start with `c`.
    pub private_key_wif: Zeroizing<String>,
    pub private_key_hex: Zeroizing<String>,
    /// ECDSA: compressed SEC encoding (02/03 prefix + x). Schnorr: x only.
    pub public_key: String,
    /// Schnorr only. An x-only key drops the Y coordinate's parity; when it is
    /// odd, BIP340 signers negate the secret key before signing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parity: Option<&'static str>,
}

/// Generates a key pair from the operating system's secure random number generator.
pub fn generate(network: Network, key_type: KeyType) -> KeyPairInfo {
    let secret = SecretKey::new(&mut rand::thread_rng());
    from_secret(secret, network, key_type)
}

/// Describes the key pair for a known secret key. Deterministic, so it can be tested.
pub fn from_secret(secret: SecretKey, network: Network, key_type: KeyType) -> KeyPairInfo {
    let secp = Secp256k1::new();
    let private_key = PrivateKey::new(secret, bitcoin::Network::from(network));

    let (public_key, parity) = match key_type {
        KeyType::Ecdsa => (private_key.public_key(&secp).to_string(), None),
        KeyType::Schnorr => {
            let (x_only, parity) = Keypair::from_secret_key(&secp, &secret).x_only_public_key();
            let parity = match parity {
                Parity::Even => "even",
                Parity::Odd => "odd",
            };
            (x_only.to_string(), Some(parity))
        }
    };

    KeyPairInfo {
        network,
        key_type,
        private_key_wif: Zeroizing::new(private_key.to_wif()),
        private_key_hex: Zeroizing::new(secret.display_secret().to_string()),
        public_key,
        parity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Secret key 1: its public key is the secp256k1 generator point G.
    fn secret_one() -> SecretKey {
        let mut bytes = [0u8; 32];
        bytes[31] = 1;
        SecretKey::from_slice(&bytes).unwrap()
    }

    const G_X: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn ecdsa_public_key_is_compressed_generator_point() {
        let info = from_secret(secret_one(), Network::Regtest, KeyType::Ecdsa);
        assert_eq!(info.public_key, format!("02{G_X}"));
        assert_eq!(info.parity, None);
    }

    #[test]
    fn mainnet_wif_matches_the_known_encoding_of_secret_one() {
        let info = from_secret(secret_one(), Network::Mainnet, KeyType::Ecdsa);
        assert_eq!(info.private_key_wif.as_str(), "KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn");
    }

    #[test]
    fn wif_uses_testnet_version_byte() {
        let info = from_secret(secret_one(), Network::Signet, KeyType::Ecdsa);
        assert_eq!(
            *info.private_key_wif,
            "cMahea7zqjxrtgAbB7LSGbcQUr1uX1ojuat9jZodMN87JcbXMTcA"
        );
    }

    #[test]
    fn schnorr_public_key_is_x_only() {
        let info = from_secret(secret_one(), Network::Testnet4, KeyType::Schnorr);
        assert_eq!(info.public_key, G_X);
        assert_eq!(info.parity, Some("even"));
    }

    #[test]
    fn generated_keys_are_unique() {
        let a = generate(Network::Regtest, KeyType::Ecdsa);
        let b = generate(Network::Regtest, KeyType::Ecdsa);
        assert_ne!(*a.private_key_hex, *b.private_key_hex);
        assert!(a.private_key_wif.starts_with('c'));
    }
}

/// A WIF private key. Its version byte says mainnet or test network.
pub fn parse_private_key(s: &str) -> Result<bitcoin::PrivateKey, crate::error::CoreError> {
    s.trim()
        .parse::<bitcoin::PrivateKey>()
        .map_err(|e| crate::error::CoreError::InvalidInput(format!("invalid WIF private key: {e}")))
}

pub fn parse_secret_key(s: &str) -> Result<bitcoin::secp256k1::SecretKey, crate::error::CoreError> {
    s.parse::<bitcoin::PrivateKey>()
        .map(|pk| pk.inner)
        .map_err(|e| crate::error::CoreError::InvalidInput(format!("invalid key: {}", e)))
}
