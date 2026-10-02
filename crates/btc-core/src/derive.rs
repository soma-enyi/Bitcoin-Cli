//! BIP32 hierarchical deterministic key derivation.
//!
//! Each child key is computed from its parent's key and chain code plus a 32-bit index:
//! - normal index (0 … 2³¹−1): uses the parent *public* key, so an xpub can derive
//!   these children too. That is how watch-only wallets generate receive addresses.
//! - hardened index (2³¹ … 2³²−1, written `0'` or `0h`): uses the parent *private*
//!   key, so only an xprv can derive them. A leaked child xprv plus the parent xpub
//!   cannot be used to recover the parent private key across a hardened step.

use std::str::FromStr;

use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv, Xpub};
use bitcoin::key::{CompressedPublicKey, Secp256k1};
use bitcoin::{NetworkKind, PrivateKey};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::address::{self, AddressType};
use crate::{CoreError, Network};

/// An extended key given as input: private (xprv/tprv) or public (xpub/tpub).
pub enum ExtendedKey {
    Private(Xpriv),
    Public(Xpub),
}

impl ExtendedKey {
    pub fn parse(s: &str) -> Result<ExtendedKey, CoreError> {
        let s = s.trim();
        if let Ok(xprv) = Xpriv::from_str(s) {
            Ok(ExtendedKey::Private(xprv))
        } else if let Ok(xpub) = Xpub::from_str(s) {
            Ok(ExtendedKey::Public(xpub))
        } else {
            Err(CoreError::InvalidInput(
                "not a valid extended key (expected xprv/xpub on mainnet, or tprv/tpub on test networks)"
                    .into(),
            ))
        }
    }

    /// Mainnet keys (xprv/xpub) and test keys (tprv/tpub) use different version bytes.
    pub fn kind(&self) -> NetworkKind {
        match self {
            ExtendedKey::Private(xprv) => xprv.network,
            ExtendedKey::Public(xpub) => xpub.network,
        }
    }

    fn xpub(&self) -> Xpub {
        match self {
            ExtendedKey::Private(xprv) => Xpub::from_priv(&Secp256k1::new(), xprv),
            ExtendedKey::Public(xpub) => *xpub,
        }
    }
}

/// Where the address type came from, shown to the user so the choice is never a surprise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressTypeSource {
    Flag,
    PathPurpose,
    Default,
}

#[derive(Serialize)]
pub struct Derivation {
    pub network: Network,
    /// `"private"` (tprv) or `"public"` (tpub).
    pub input_kind: &'static str,
    pub input_depth: u8,
    pub input_fingerprint: String,
    pub address_type: AddressType,
    pub address_type_source: AddressTypeSource,
    pub children: Vec<DerivedKey>,
    /// Non-fatal problems with the request, e.g. a mainnet coin type.
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct DerivedKey {
    /// Relative to the input key: `m/…` when the input is a master key.
    pub path: String,
    pub depth: u8,
    pub address: String,
    pub public_key: String,
    pub xpub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xprv: Option<Zeroizing<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_wif: Option<Zeroizing<String>>,
}

impl Derivation {
    pub fn contains_secrets(&self) -> bool {
        self.input_kind == "private"
    }
}

/// Accepts `m/84'/1'/0'/0/0`, `m/84h/1h/0h/0/0` or a relative `0/0`.
pub fn parse_path(s: &str) -> Result<DerivationPath, CoreError> {
    DerivationPath::from_str(s.trim())
        .map_err(|e| CoreError::InvalidInput(format!("invalid derivation path `{s}`: {e}")))
}

/// Derives `count` consecutive children: the path as given, then the last index + 1, + 2, …
pub fn derive(
    key: &ExtendedKey,
    path: &DerivationPath,
    count: u32,
    address_type: Option<AddressType>,
    network: Network,
) -> Result<Derivation, CoreError> {
    let secp = Secp256k1::new();
    let steps: &[ChildNumber] = path.as_ref();
    let input_xpub = key.xpub();

    let key_is_mainnet = key.kind() == NetworkKind::Main;
    if key_is_mainnet != network.is_mainnet() {
        let (have, want) = if key_is_mainnet {
            ("a mainnet key (xprv/xpub)", "pass --network mainnet")
        } else {
            (
                "a test-network key (tprv/tpub)",
                "use a test network, or an xprv/xpub for mainnet",
            )
        };
        return Err(CoreError::InvalidInput(format!(
            "this is {have} but the network is {network}: {want}"
        )));
    }

    if count == 0 {
        return Err(CoreError::InvalidInput("--count must be at least 1".into()));
    }
    let Some((&first_last, base)) = steps.split_last() else {
        if count > 1 {
            return Err(CoreError::InvalidInput(
                "--count needs a path with at least one index to increment".into(),
            ));
        }
        return describe_single(key, path, address_type, network);
    };

    if let ExtendedKey::Public(_) = key {
        if let Some(hardened) = steps.iter().find(|c| c.is_hardened()) {
            return Err(CoreError::InvalidInput(format!(
                "cannot derive hardened index {hardened} from a tpub: hardened derivation \
                 needs the private key. Use a tprv, or a path of normal indexes only"
            )));
        }
    }

    let (address_type, address_type_source) =
        resolve_address_type(address_type, steps, input_xpub.depth);

    // Derive the shared parent once, then each child from it.
    let parent = derive_path(key, base)?;
    let mut children = Vec::with_capacity(count as usize);
    let mut child_number = first_last;
    for i in 0..count {
        if i > 0 {
            child_number = child_number.increment().map_err(|_| {
                CoreError::InvalidInput("--count runs past the last possible index".into())
            })?;
        }
        let mut child_path = base.to_vec();
        child_path.push(child_number);
        let child = derive_path(&parent, &[child_number])?;
        children.push(describe_child(
            &secp,
            &child,
            &child_path,
            input_xpub.depth,
            address_type,
            network,
        ));
    }

    Ok(Derivation {
        network,
        input_kind: input_kind(key),
        input_depth: input_xpub.depth,
        input_fingerprint: input_xpub.fingerprint().to_string(),
        address_type,
        address_type_source,
        warnings: warnings(steps, input_xpub.depth, network),
        children,
    })
}

/// `derive <key> m`: describe the input key itself.
fn describe_single(
    key: &ExtendedKey,
    path: &DerivationPath,
    address_type: Option<AddressType>,
    network: Network,
) -> Result<Derivation, CoreError> {
    let secp = Secp256k1::new();
    let input_xpub = key.xpub();
    let (address_type, address_type_source) =
        resolve_address_type(address_type, path.as_ref(), input_xpub.depth);
    let child = derive_path(key, &[])?;
    Ok(Derivation {
        network,
        input_kind: input_kind(key),
        input_depth: input_xpub.depth,
        input_fingerprint: input_xpub.fingerprint().to_string(),
        address_type,
        address_type_source,
        children: vec![describe_child(
            &secp,
            &child,
            &[],
            input_xpub.depth,
            address_type,
            network,
        )],
        warnings: Vec::new(),
    })
}

fn derive_path(key: &ExtendedKey, steps: &[ChildNumber]) -> Result<ExtendedKey, CoreError> {
    let secp = Secp256k1::new();
    Ok(match key {
        ExtendedKey::Private(xprv) => ExtendedKey::Private(xprv.derive_priv(&secp, &steps)?),
        ExtendedKey::Public(xpub) => ExtendedKey::Public(xpub.derive_pub(&secp, &steps)?),
    })
}

fn describe_child(
    secp: &Secp256k1<bitcoin::secp256k1::All>,
    child: &ExtendedKey,
    path: &[ChildNumber],
    input_depth: u8,
    address_type: AddressType,
    network: Network,
) -> DerivedKey {
    let xpub = child.xpub();
    let public_key = CompressedPublicKey(xpub.public_key);
    let (xprv, private_key_wif) = match child {
        ExtendedKey::Private(xprv) => (
            Some(Zeroizing::new(xprv.to_string())),
            Some(Zeroizing::new(
                PrivateKey::new(xprv.private_key, bitcoin::Network::from(network)).to_wif(),
            )),
        ),
        ExtendedKey::Public(_) => (None, None),
    };

    DerivedKey {
        path: format_path(path, input_depth),
        depth: xpub.depth,
        address: address::from_public_key(secp, &public_key, address_type, network).to_string(),
        public_key: public_key.to_string(),
        xpub: xpub.to_string(),
        xprv,
        private_key_wif,
    }
}

/// Precedence: explicit flag, then the path's `purpose'` (only meaningful from a
/// master key), then P2WPKH.
fn resolve_address_type(
    flag: Option<AddressType>,
    steps: &[ChildNumber],
    input_depth: u8,
) -> (AddressType, AddressTypeSource) {
    if let Some(address_type) = flag {
        return (address_type, AddressTypeSource::Flag);
    }
    if input_depth == 0 {
        if let Some(ChildNumber::Hardened { index }) = steps.first() {
            if let Some(address_type) = AddressType::from_purpose(*index) {
                return (address_type, AddressTypeSource::PathPurpose);
            }
        }
    }
    (AddressType::P2wpkh, AddressTypeSource::Default)
}

/// BIP44 reserves coin type 0' for mainnet and 1' for every test network.
fn warnings(steps: &[ChildNumber], input_depth: u8, network: Network) -> Vec<String> {
    let mut warnings = Vec::new();
    if input_depth == 0 {
        if let [ChildNumber::Hardened { index: purpose }, coin, ..] = steps {
            let expected: u32 = if network.is_mainnet() { 0 } else { 1 };
            if AddressType::from_purpose(*purpose).is_some()
                && *coin != (ChildNumber::Hardened { index: expected })
            {
                let (name, other) = if network.is_mainnet() {
                    ("mainnet", "1' is for test networks")
                } else {
                    ("test networks", "0' is mainnet")
                };
                warnings.push(format!(
                    "coin type {coin} in a BIP{purpose} path: {name} conventionally use {expected}' \
                     ({other}), so other wallets won't find these addresses"
                ));
            }
        }
    }
    warnings
}

fn format_path(path: &[ChildNumber], input_depth: u8) -> String {
    let indexes: Vec<String> = path.iter().map(ChildNumber::to_string).collect();
    match (input_depth, indexes.is_empty()) {
        (0, true) => "m".into(),
        (0, false) => format!("m/{}", indexes.join("/")),
        (_, true) => ".".into(),
        (_, false) => indexes.join("/"),
    }
}

fn input_kind(key: &ExtendedKey) -> &'static str {
    match key {
        ExtendedKey::Private(_) => "private",
        ExtendedKey::Public(_) => "public",
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::Address;

    use super::*;
    use crate::mnemonic;

    const ABANDON_ABOUT: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn abandon_root() -> ExtendedKey {
        let root = mnemonic::root_key(ABANDON_ABOUT, "", Network::Regtest).unwrap();
        ExtendedKey::parse(&root.xprv).unwrap()
    }

    fn derive_one(key: &ExtendedKey, path: &str, network: Network) -> DerivedKey {
        let path = parse_path(path).unwrap();
        derive(key, &path, 1, None, network)
            .unwrap()
            .children
            .remove(0)
    }

    /// Compares by scriptPubKey so a mainnet vector can check a test-network address.
    fn assert_same_script(ours: &str, expected: &str) {
        let script = |a: &str| {
            Address::from_str(a)
                .unwrap()
                .assume_checked()
                .script_pubkey()
        };
        assert_eq!(script(ours), script(expected), "{ours} vs {expected}");
    }

    /// Converts a mainnet vector xprv into the tprv we accept (same key material).
    fn as_test_key(mainnet_xprv: &str) -> ExtendedKey {
        let mut xprv = Xpriv::from_str(mainnet_xprv).unwrap();
        xprv.network = NetworkKind::Test;
        ExtendedKey::Private(xprv)
    }

    #[test]
    fn bip32_test_vector_1() {
        // https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki#test-vector-1
        let master = as_test_key(
            "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi",
        );
        let vectors = [
            (
                "m/0'",
                "xprv9uHRZZhk6KAJC1avXpDAp4MDc3sQKNxDiPvvkX8Br5ngLNv1TxvUxt4cV1rGL5hj6KCesnDYUhd7oWgT11eZG7XnxHrnYeSvkzY7d2bhkJ7",
            ),
            (
                "m/0'/1",
                "xprv9wTYmMFdV23N2TdNG573QoEsfRrWKQgWeibmLntzniatZvR9BmLnvSxqu53Kw1UmYPxLgboyZQaXwTCg8MSY3H2EU4pWcQDnRnrVA1xe8fs",
            ),
            (
                "m/0'/1/2'",
                "xprv9z4pot5VBttmtdRTWfWQmoH1taj2axGVzFqSb8C9xaxKymcFzXBDptWmT7FwuEzG3ryjH4ktypQSAewRiNMjANTtpgP4mLTj34bhnZX7UiM",
            ),
            (
                "m/0'/1/2'/2",
                "xprvA2JDeKCSNNZky6uBCviVfJSKyQ1mDYahRjijr5idH2WwLsEd4Hsb2Tyh8RfQMuPh7f7RtyzTtdrbdqqsunu5Mm3wDvUAKRHSC34sJ7in334",
            ),
            (
                "m/0'/1/2'/2/1000000000",
                "xprvA41z7zogVVwxVSgdKUHDy1SKmdb533PjDz7J6N6mV6uS3ze1ai8FHa8kmHScGpWmj4WggLyQjgPie1rFSruoUihUZREPSL39UNdE3BBDu76",
            ),
        ];
        for (path, expected) in vectors {
            let child = derive_one(&master, path, Network::Regtest);
            let ours = Xpriv::from_str(child.xprv.as_deref().unwrap()).unwrap();
            let expected = Xpriv::from_str(expected).unwrap();
            assert_eq!(ours.private_key, expected.private_key, "{path}");
            assert_eq!(ours.chain_code, expected.chain_code, "{path}");
            assert_eq!(ours.depth, expected.depth, "{path}");
            assert_eq!(child.path, path);
        }
    }

    #[test]
    fn bip44_49_84_86_address_vectors() {
        // First receive address of the "abandon … about" mnemonic in each BIP's test vectors.
        let root = abandon_root();
        let cases = [
            ("m/44'/0'/0'/0/0", "1LqBGSKuX5yYUonjxT5qGfpUsXKYYWeabA"),
            (
                "m/84'/0'/0'/0/0",
                "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
            ),
            (
                "m/86'/0'/0'/0/0",
                "bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr",
            ),
        ];
        for (path, expected) in cases {
            let child = derive_one(&root, path, Network::Signet);
            assert_same_script(&child.address, expected);
        }
        // BIP49's vector is already a testnet address, so it can be compared directly.
        let child = derive_one(&root, "m/49'/1'/0'/0/0", Network::Testnet4);
        assert_eq!(child.address, "2Mww8dCYPUpKHofjgcXcBCEGmniw9CoaiD2");
    }

    #[test]
    fn address_type_follows_the_path_purpose() {
        let root = abandon_root();
        let path = parse_path("m/86'/1'/0'/0/0").unwrap();
        let derivation = derive(&root, &path, 1, None, Network::Regtest).unwrap();
        assert_eq!(derivation.address_type, AddressType::P2tr);
        assert_eq!(
            derivation.address_type_source,
            AddressTypeSource::PathPurpose
        );
        assert!(derivation.children[0].address.starts_with("bcrt1p"));

        let forced = derive(&root, &path, 1, Some(AddressType::P2pkh), Network::Regtest).unwrap();
        assert_eq!(forced.address_type_source, AddressTypeSource::Flag);
    }

    #[test]
    fn watch_only_xpub_derives_the_same_addresses() {
        // A wallet exports its account tpub; the tpub alone must produce the same
        // receive addresses as the full private derivation.
        let root = abandon_root();
        let account = derive_one(&root, "m/84'/1'/0'", Network::Regtest);
        let account_xpub = ExtendedKey::parse(&account.xpub).unwrap();

        let from_root = derive_one(&root, "m/84'/1'/0'/0/7", Network::Regtest);
        let from_xpub = derive_one(&account_xpub, "0/7", Network::Regtest);
        assert_eq!(from_root.address, from_xpub.address);
        assert_eq!(from_xpub.path, "0/7");
        assert!(from_xpub.xprv.is_none());
    }

    #[test]
    fn xpub_cannot_derive_hardened_children() {
        let root = abandon_root();
        let xpub = ExtendedKey::parse(&root.xpub().to_string()).unwrap();
        let path = parse_path("m/84'/1'/0'").unwrap();
        let err = derive(&xpub, &path, 1, None, Network::Regtest)
            .err()
            .unwrap();
        assert!(err.to_string().contains("hardened"));
    }

    #[test]
    fn count_increments_the_last_index() {
        let root = abandon_root();
        let path = parse_path("m/84h/1h/0h/0/5").unwrap();
        let derivation = derive(&root, &path, 3, None, Network::Regtest).unwrap();
        let paths: Vec<_> = derivation
            .children
            .iter()
            .map(|c| c.path.as_str())
            .collect();
        assert_eq!(
            paths,
            ["m/84'/1'/0'/0/5", "m/84'/1'/0'/0/6", "m/84'/1'/0'/0/7"]
        );
    }

    #[test]
    fn mainnet_coin_type_is_warned_about() {
        let root = abandon_root();
        let path = parse_path("m/84'/0'/0'/0/0").unwrap();
        let derivation = derive(&root, &path, 1, None, Network::Regtest).unwrap();
        assert_eq!(derivation.warnings.len(), 1);
        let path = parse_path("m/84'/1'/0'/0/0").unwrap();
        let derivation = derive(&root, &path, 1, None, Network::Regtest).unwrap();
        assert!(derivation.warnings.is_empty());
    }

    fn mainnet_root() -> ExtendedKey {
        let root = mnemonic::root_key(ABANDON_ABOUT, "", Network::Mainnet).unwrap();
        ExtendedKey::parse(&root.xprv).unwrap()
    }

    #[test]
    fn mainnet_root_matches_the_bip39_vector() {
        let root = mnemonic::root_key(ABANDON_ABOUT, "", Network::Mainnet).unwrap();
        assert_eq!(
            root.xprv.as_str(),
            "xprv9s21ZrQH143K3GJpoapnV8SFfukcVBSfeCficPSGfubmSFDxo1kuHnLisriDvSnRRuL2Qrg5ggqHKNVpxR86QEC8w35uxmGoggxtQTPvfUu"
        );
    }

    #[test]
    fn mainnet_addresses_match_the_bip_vectors_exactly() {
        let root = mainnet_root();
        let cases = [
            ("m/44'/0'/0'/0/0", "1LqBGSKuX5yYUonjxT5qGfpUsXKYYWeabA"),
            ("m/49'/0'/0'/0/0", "37VucYSaXLCAsxYyAPfbSi9eh4iEcbShgf"),
            (
                "m/84'/0'/0'/0/0",
                "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
            ),
            (
                "m/86'/0'/0'/0/0",
                "bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr",
            ),
        ];
        for (path, expected) in cases {
            let child = derive_one(&root, path, Network::Mainnet);
            assert_eq!(child.address, expected, "{path}");
        }
    }

    #[test]
    fn mainnet_private_keys_use_the_mainnet_wif_prefix() {
        let child = derive_one(&mainnet_root(), "m/84'/0'/0'/0/0", Network::Mainnet);
        let wif = child.private_key_wif.expect("an xprv gives a WIF");
        assert!(
            wif.starts_with('K') || wif.starts_with('L'),
            "{}",
            wif.as_str()
        );
    }

    #[test]
    fn a_mainnet_key_is_refused_on_a_test_network_and_vice_versa() {
        let path = parse_path("m/84'/0'/0'/0/0").unwrap();
        let err = derive(&mainnet_root(), &path, 1, None, Network::Regtest)
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("mainnet key") && err.contains("--network mainnet"),
            "{err}"
        );

        let err = derive(&abandon_root(), &path, 1, None, Network::Mainnet)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("test-network key"), "{err}");
    }

    #[test]
    fn coin_type_warning_follows_the_network() {
        let root = mainnet_root();
        let path = parse_path("m/84'/0'/0'/0/0").unwrap();
        assert!(
            derive(&root, &path, 1, None, Network::Mainnet)
                .unwrap()
                .warnings
                .is_empty()
        );
        let path = parse_path("m/84'/1'/0'/0/0").unwrap();
        assert_eq!(
            derive(&root, &path, 1, None, Network::Mainnet)
                .unwrap()
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn parses_both_mainnet_and_test_extended_keys() {
        assert!(ExtendedKey::parse(
            "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi"
        )
        .is_ok());
        assert!(ExtendedKey::parse("not a key").is_err());
    }
}
