//! Building addresses from public keys, and validating addresses.

use std::str::FromStr;

use bitcoin::address::NetworkUnchecked;
use bitcoin::hex::DisplayHex;
use bitcoin::key::{CompressedPublicKey, PublicKey, Secp256k1, Verification, XOnlyPublicKey};
use bitcoin::{Address, KnownHrp};
use serde::Serialize;

use crate::{CoreError, Network};

/// The standard single-key address types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddressType {
    /// Legacy: base58, pays to HASH160(pubkey). Starts with `m`/`n` on test networks.
    P2pkh,
    /// Nested SegWit: a P2WPKH script wrapped in P2SH for old wallets. Starts with `2`.
    P2shP2wpkh,
    /// Native SegWit v0: bech32. Starts with `tb1q` (`bcrt1q` on regtest).
    P2wpkh,
    /// Taproot, SegWit v1: bech32m, BIP86 key-path only. Starts with `tb1p` (`bcrt1p`).
    P2tr,
}

impl AddressType {
    pub const ALL: [AddressType; 4] = [
        AddressType::P2pkh,
        AddressType::P2shP2wpkh,
        AddressType::P2wpkh,
        AddressType::P2tr,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AddressType::P2pkh => "p2pkh",
            AddressType::P2shP2wpkh => "p2sh-p2wpkh",
            AddressType::P2wpkh => "p2wpkh",
            AddressType::P2tr => "p2tr",
        }
    }

    /// The address type a BIP44-style path implies through its `purpose'` field.
    pub fn from_purpose(purpose: u32) -> Option<AddressType> {
        match purpose {
            44 => Some(AddressType::P2pkh),
            49 => Some(AddressType::P2shP2wpkh),
            84 => Some(AddressType::P2wpkh),
            86 => Some(AddressType::P2tr),
            _ => None,
        }
    }

    pub fn purpose(self) -> u32 {
        match self {
            AddressType::P2pkh => 44,
            AddressType::P2shP2wpkh => 49,
            AddressType::P2wpkh => 84,
            AddressType::P2tr => 86,
        }
    }
}

/// Builds the address of `address_type` that pays to `public_key` on `network`.
pub fn from_public_key<C: Verification>(
    secp: &Secp256k1<C>,
    public_key: &CompressedPublicKey,
    address_type: AddressType,
    network: Network,
) -> Address {
    let network = bitcoin::Network::from(network);
    match address_type {
        AddressType::P2pkh => Address::p2pkh(public_key, network),
        AddressType::P2shP2wpkh => Address::p2shwpkh(public_key, network),
        AddressType::P2wpkh => Address::p2wpkh(public_key, KnownHrp::from(network)),
        // BIP86: the x-only key is tweaked with an empty script tree, so the
        // output can only be spent with a key-path signature.
        AddressType::P2tr => Address::p2tr(
            secp,
            XOnlyPublicKey::from(public_key.0),
            None,
            KnownHrp::from(network),
        ),
    }
}

// ---------------------------------------------------------------------------
// address from-pubkey
// ---------------------------------------------------------------------------

/// The three public key encodings, which limit the address types they can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyFormat {
    /// 33 bytes, `02`/`03` + x. Works for every address type.
    Compressed,
    /// 65 bytes, `04` + x + y. Pre-2012 format; SegWit forbids it, so P2PKH only.
    Uncompressed,
    /// 32 bytes, x only (BIP340). Taproot only: the Y parity other types need is gone.
    XOnly,
}

enum ParsedKey {
    Full(PublicKey),
    XOnly(XOnlyPublicKey),
}

#[derive(Serialize)]
pub struct PubkeyAddresses {
    pub network: Network,
    pub public_key: String,
    pub key_format: KeyFormat,
    pub addresses: Vec<DerivedAddress>,
    /// Types that were requested (via `all`) but this key format can't produce.
    pub unsupported: Vec<Unsupported>,
}

#[derive(Serialize)]
pub struct DerivedAddress {
    pub address_type: AddressType,
    pub address: String,
    pub script_pubkey: String,
}

#[derive(Serialize)]
pub struct Unsupported {
    pub address_type: AddressType,
    pub reason: &'static str,
}

/// Builds addresses for a hex public key. `None` means every type the key supports;
/// a specific type the key can't produce is an error.
pub fn from_pubkey_hex(
    hex: &str,
    address_type: Option<AddressType>,
    network: Network,
) -> Result<PubkeyAddresses, CoreError> {
    let secp = Secp256k1::verification_only();
    let hex = hex.trim();
    let (key, key_format) = parse_public_key(hex)?;

    let mut addresses = Vec::new();
    let mut unsupported = Vec::new();
    let requested: &[AddressType] = match &address_type {
        Some(t) => std::slice::from_ref(t),
        None => &AddressType::ALL,
    };
    for &address_type in requested {
        match build(&secp, &key, address_type, network) {
            Ok(address) => addresses.push(DerivedAddress {
                address_type,
                script_pubkey: address.script_pubkey().to_hex_string(),
                address: address.to_string(),
            }),
            Err(reason) if requested.len() == 1 => {
                return Err(CoreError::InvalidInput(format!(
                    "cannot make a {} address from a {} key: {reason}",
                    address_type.as_str(),
                    key_format_name(key_format)
                )));
            }
            Err(reason) => unsupported.push(Unsupported {
                address_type,
                reason,
            }),
        }
    }

    Ok(PubkeyAddresses {
        network,
        public_key: hex.to_lowercase(),
        key_format,
        addresses,
        unsupported,
    })
}

fn parse_public_key(hex: &str) -> Result<(ParsedKey, KeyFormat), CoreError> {
    let invalid =
        |e: &dyn std::fmt::Display| CoreError::InvalidInput(format!("invalid public key: {e}"));
    match hex.len() {
        64 => {
            let key = XOnlyPublicKey::from_str(hex).map_err(|e| invalid(&e))?;
            Ok((ParsedKey::XOnly(key), KeyFormat::XOnly))
        }
        66 | 130 => {
            let key = PublicKey::from_str(hex).map_err(|e| invalid(&e))?;
            let format = if key.compressed {
                KeyFormat::Compressed
            } else {
                KeyFormat::Uncompressed
            };
            Ok((ParsedKey::Full(key), format))
        }
        n => Err(CoreError::InvalidInput(format!(
            "invalid public key: expected 64 (x-only), 66 (compressed) or 130 (uncompressed) \
             hex characters, got {n}"
        ))),
    }
}

fn build<C: Verification>(
    secp: &Secp256k1<C>,
    key: &ParsedKey,
    address_type: AddressType,
    network: Network,
) -> Result<Address, &'static str> {
    match key {
        ParsedKey::Full(key) if key.compressed => Ok(from_public_key(
            secp,
            &CompressedPublicKey(key.inner),
            address_type,
            network,
        )),
        ParsedKey::Full(key) => match address_type {
            AddressType::P2pkh => Ok(Address::p2pkh(*key, bitcoin::Network::from(network))),
            _ => Err("SegWit and Taproot only accept compressed public keys"),
        },
        ParsedKey::XOnly(key) => match address_type {
            AddressType::P2tr => Ok(Address::p2tr(
                secp,
                *key,
                None,
                KnownHrp::from(bitcoin::Network::from(network)),
            )),
            _ => Err("an x-only key has lost its Y parity, which only Taproot can do without"),
        },
    }
}

fn key_format_name(format: KeyFormat) -> &'static str {
    match format {
        KeyFormat::Compressed => "compressed",
        KeyFormat::Uncompressed => "uncompressed",
        KeyFormat::XOnly => "x-only",
    }
}

// ---------------------------------------------------------------------------
// address validate
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AddressReport {
    pub address: String,
    pub network: Network,
    /// Every network the address is valid on (test networks share prefixes).
    pub valid_networks: Vec<&'static str>,
    /// p2pkh, p2sh, p2wpkh, p2wsh, p2tr, p2a, or `witness_v<N>` for future versions.
    pub address_type: String,
    /// base58check, bech32 (witness v0) or bech32m (witness v1+, BIP350).
    pub encoding: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witness_version: Option<u8>,
    /// What the output commits to: a key hash, script hash, witness program or
    /// Taproot output key.
    pub payload: String,
    pub script_pubkey: String,
    pub script_pubkey_asm: String,
    pub notes: Vec<&'static str>,
}

/// Parses and checksums an address, and checks it belongs to `network`.
pub fn validate(input: &str, network: Network) -> Result<AddressReport, CoreError> {
    let input = input.trim();
    let unchecked = Address::<NetworkUnchecked>::from_str(input).map_err(|e| {
        // rust-bitcoin falls back to base58 when bech32 decoding fails, so a bad
        // bech32 address would report a confusing base58 error. Show the real one.
        let lower = input.to_ascii_lowercase();
        let looks_bech32 = ["bc1", "tb1", "bcrt1"].iter().any(|p| lower.starts_with(p));
        let detail = match bitcoin::bech32::segwit::decode(input) {
            Err(bech32_error) if looks_bech32 => match checksum_variant_hint(input) {
                Some(hint) => hint.to_owned(),
                None => error_chain(&bech32_error),
            },
            _ => error_chain(&e),
        };
        CoreError::InvalidAddress(detail)
    })?;

    let mut valid_networks = Vec::new();
    for n in Network::ALL {
        if unchecked.is_valid_for_network(n.into()) {
            valid_networks.push(n.as_str());
        }
    }
    if !valid_networks.contains(&network.as_str()) {
        return Err(CoreError::WrongNetwork {
            address: input.to_owned(),
            actual: valid_networks.join("/"),
            selected: network.to_string(),
        });
    }

    let address = unchecked.assume_checked();
    let script_pubkey = address.script_pubkey();
    let witness_program = address.witness_program();
    let witness_version = witness_program.map(|p| p.version().to_num());

    let address_type = match (address.address_type(), witness_version) {
        (Some(t), _) => t.to_string(),
        (None, Some(v)) => format!("witness_v{v}"),
        (None, None) => "unknown".to_owned(),
    };
    let encoding = match witness_version {
        None => "base58check",
        Some(0) => "bech32",
        Some(_) => "bech32m",
    };
    let payload = if let Some(program) = witness_program {
        program.program().as_bytes().to_lower_hex_string()
    } else if let Some(hash) = address.pubkey_hash() {
        hash.to_string()
    } else if let Some(hash) = address.script_hash() {
        hash.to_string()
    } else {
        String::new()
    };

    let mut notes = Vec::new();
    match address_type.as_str() {
        "p2sh" => notes.push(
            "P2SH commits to a script hash; the script (e.g. P2SH-P2WPKH or multisig) \
             is only revealed when the output is spent",
        ),
        "p2tr" => notes.push(
            "the payload is the tweaked Taproot output key; whether it also commits to \
             a script tree can't be told from the address",
        ),
        "p2wsh" => notes.push("the payload is the SHA256 of the witness script"),
        _ => {}
    }
    if witness_version.is_some_and(|v| v > 1) {
        notes.push("future witness version: anyone can spend it until a soft fork defines it");
    }
    if witness_version.is_some() && input.chars().any(|c| c.is_ascii_uppercase()) {
        notes.push("all-uppercase bech32 is valid (it makes QR codes smaller)");
    }

    Ok(AddressReport {
        address: address.to_string(),
        network,
        valid_networks,
        address_type,
        encoding,
        witness_version,
        payload,
        script_pubkey: script_pubkey.to_hex_string(),
        script_pubkey_asm: script_pubkey.to_asm_string(),
        notes,
    })
}

/// BIP350: witness v0 must use the bech32 checksum and v1+ must use bech32m.
/// When the checksum is valid under the *other* variant, say so.
fn checksum_variant_hint(input: &str) -> Option<&'static str> {
    use bitcoin::bech32::primitives::decode::CheckedHrpstring;
    use bitcoin::bech32::{Bech32, Bech32m};

    let bech32 = CheckedHrpstring::new::<Bech32>(input).is_ok();
    let bech32m = CheckedHrpstring::new::<Bech32m>(input).is_ok();
    match (bech32, bech32m) {
        (true, false) => {
            Some("the checksum is bech32, but witness v1+ addresses must use bech32m (BIP350)")
        }
        (false, true) => {
            Some("the checksum is bech32m, but witness v0 addresses must use bech32 (BIP173)")
        }
        _ => None,
    }
}

/// Joins an error with its causes: the top-level messages of these parse errors are
/// generic ("base58 error"), the useful part is in `source()`.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    const G_COMPRESSED: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const G_UNCOMPRESSED: &str = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";
    const G_X_ONLY: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn script_of(address: &str) -> String {
        Address::from_str(address)
            .unwrap()
            .assume_checked()
            .script_pubkey()
            .to_hex_string()
    }

    fn address_of(set: &PubkeyAddresses, t: AddressType) -> &DerivedAddress {
        set.addresses.iter().find(|a| a.address_type == t).unwrap()
    }

    #[test]
    fn compressed_key_makes_every_type() {
        let set = from_pubkey_hex(G_COMPRESSED, None, Network::Regtest).unwrap();
        assert_eq!(set.addresses.len(), 4);
        assert!(set.unsupported.is_empty());
        // Well-known mainnet addresses of the generator point, compared by script.
        let p2pkh = address_of(&set, AddressType::P2pkh);
        assert_eq!(
            p2pkh.script_pubkey,
            script_of("1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH")
        );
        // The BIP173 example address is exactly HASH160(G compressed).
        let p2wpkh = address_of(&set, AddressType::P2wpkh);
        assert_eq!(
            p2wpkh.script_pubkey,
            script_of("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
        );
        assert!(p2wpkh.address.starts_with("bcrt1q"));
    }

    #[test]
    fn uncompressed_key_only_makes_p2pkh() {
        let set = from_pubkey_hex(G_UNCOMPRESSED, None, Network::Signet).unwrap();
        assert_eq!(set.key_format, KeyFormat::Uncompressed);
        assert_eq!(set.addresses.len(), 1);
        assert_eq!(
            set.addresses[0].script_pubkey,
            script_of("1EHNa6Q4Jz2uvNExL497mE43ikXhwF6kZm")
        );
        assert_eq!(set.unsupported.len(), 3);
    }

    #[test]
    fn x_only_key_only_makes_p2tr_matching_the_compressed_key() {
        let x_only = from_pubkey_hex(G_X_ONLY, None, Network::Regtest).unwrap();
        assert_eq!(x_only.addresses.len(), 1);
        let compressed =
            from_pubkey_hex(G_COMPRESSED, Some(AddressType::P2tr), Network::Regtest).unwrap();
        assert_eq!(x_only.addresses[0].address, compressed.addresses[0].address);
    }

    #[test]
    fn explicit_unsupported_type_is_an_error() {
        let err = from_pubkey_hex(G_X_ONLY, Some(AddressType::P2wpkh), Network::Regtest)
            .err()
            .unwrap();
        assert!(err.to_string().contains("x-only"));
    }

    #[test]
    fn rejects_malformed_keys() {
        assert!(from_pubkey_hex("02abcd", None, Network::Regtest).is_err());
        // Right length, but x is not on the curve.
        let off_curve = format!("02{}", "f".repeat(64));
        assert!(from_pubkey_hex(&off_curve, None, Network::Regtest).is_err());
    }

    #[test]
    fn validates_bip173_and_bip350_testnet_vectors() {
        let p2wsh = validate(
            "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7",
            Network::Testnet4,
        )
        .unwrap();
        assert_eq!(p2wsh.address_type, "p2wsh");
        assert_eq!(p2wsh.encoding, "bech32");
        assert_eq!(p2wsh.witness_version, Some(0));

        let p2tr = validate(
            "tb1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c",
            Network::Signet,
        )
        .unwrap();
        assert_eq!(p2tr.address_type, "p2tr");
        assert_eq!(p2tr.encoding, "bech32m");
        assert_eq!(p2tr.valid_networks, ["signet", "testnet4"]);
    }

    #[test]
    fn rejects_the_wrong_checksum_variant() {
        // BIP350 invalid vectors: v2 with a bech32 checksum, v0 with a bech32m checksum.
        for address in [
            "tb1z0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqglt7rf",
            "tb1q0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vq24jc47",
        ] {
            let err = validate(address, Network::Signet)
                .err()
                .unwrap()
                .to_string();
            assert!(err.contains("bech32"), "{address}: {err}");
        }
    }

    #[test]
    fn rejects_a_single_changed_character() {
        let mutated = "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k8";
        let err = validate(mutated, Network::Signet)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("checksum"), "{err}");
    }

    #[test]
    fn mainnet_addresses_validate_on_mainnet_only() {
        let report = validate(
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
            Network::Mainnet,
        )
        .unwrap();
        assert_eq!(report.valid_networks, ["mainnet"]);
        let err = validate(
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
            Network::Signet,
        )
        .err()
        .unwrap();
        assert!(matches!(err, CoreError::WrongNetwork { .. }));
        let err = validate(
            "bcrt1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
            Network::Mainnet,
        )
        .err();
        assert!(err.is_some());
    }

    #[test]
    fn mainnet_and_cross_network_addresses_are_rejected() {
        let err = validate(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
            Network::Regtest,
        )
        .err()
        .unwrap();
        assert!(matches!(err, CoreError::WrongNetwork { .. }));
        assert!(err.to_string().contains("mainnet"));
        // tb1 is signet/testnet4, but regtest uses bcrt1.
        assert!(
            validate(
                "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7",
                Network::Regtest
            )
            .is_err()
        );
    }

    #[test]
    fn base58_test_prefixes_are_shared_with_regtest() {
        let set = from_pubkey_hex(G_COMPRESSED, Some(AddressType::P2pkh), Network::Signet).unwrap();
        let report = validate(&set.addresses[0].address, Network::Regtest).unwrap();
        assert_eq!(report.encoding, "base58check");
        assert_eq!(report.valid_networks, ["regtest", "signet", "testnet4"]);
        assert_eq!(report.payload, "751e76e8199196d454941c45d1b3a323f1433bd6");
    }

    #[test]
    fn uppercase_bech32_is_valid() {
        let upper = "TB1QRP33G0Q5C5TXSP9ARYSRX4K6ZDKFS4NCE4XJ0GDCCCEFVPYSXF3Q0SL5K7";
        let report = validate(upper, Network::Signet).unwrap();
        assert!(report.notes.iter().any(|n| n.contains("uppercase")));
    }
}
