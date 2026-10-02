//! BIP39 mnemonics and the BIP32 root key they produce.
//!
//! entropy (128–256 bits) + checksum (first entropy/32 bits of SHA256(entropy))
//!   → split into 11-bit groups → one word per group (2048-word list)
//!   → PBKDF2-HMAC-SHA512(mnemonic, "mnemonic" + passphrase, 2048 rounds) = 64-byte seed
//!   → HMAC-SHA512("Bitcoin seed", seed) = master private key + chain code (BIP32)

use bip39::Mnemonic;
use bitcoin::bip32::{Xpriv, Xpub};
use bitcoin::secp256k1::Secp256k1;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{CoreError, Network};

pub const WORD_COUNTS: [usize; 5] = [12, 15, 18, 21, 24];

/// A new mnemonic plus the root key it produces.
#[derive(Serialize)]
pub struct MnemonicInfo {
    pub mnemonic: Zeroizing<String>,
    pub word_count: usize,
    pub entropy_bits: usize,
    pub checksum_bits: usize,
    pub passphrase_used: bool,
    pub root: RootKey,
}

/// The BIP32 master key (depth 0) derived from a mnemonic's seed.
#[derive(Serialize)]
pub struct RootKey {
    pub network: Network,
    /// First 4 bytes of HASH160(master public key). Wallets and PSBTs use it to
    /// recognise which seed a key belongs to.
    pub fingerprint: String,
    /// `tprv…` on every test network.
    pub xprv: Zeroizing<String>,
    pub xpub: String,
}

/// Generates a mnemonic from fresh random entropy.
pub fn generate(
    word_count: usize,
    passphrase: &str,
    network: Network,
) -> Result<MnemonicInfo, CoreError> {
    let mnemonic = Mnemonic::generate(word_count)?;
    describe(&mnemonic, passphrase, network)
}

/// Builds the mnemonic for known entropy. Deterministic, so it can be tested.
pub fn from_entropy(
    entropy: &[u8],
    passphrase: &str,
    network: Network,
) -> Result<MnemonicInfo, CoreError> {
    let mnemonic = Mnemonic::from_entropy(entropy)?;
    describe(&mnemonic, passphrase, network)
}

/// Parses a mnemonic, checking every word and the checksum, and returns its root key.
pub fn root_key(phrase: &str, passphrase: &str, network: Network) -> Result<RootKey, CoreError> {
    let phrase = Zeroizing::new(phrase.to_lowercase());
    let mnemonic = Mnemonic::parse(phrase.as_str())?;
    root_from_mnemonic(&mnemonic, passphrase, network)
}

fn describe(
    mnemonic: &Mnemonic,
    passphrase: &str,
    network: Network,
) -> Result<MnemonicInfo, CoreError> {
    let word_count = mnemonic.word_count();
    let entropy_bits = word_count * 11 * 32 / 33;

    Ok(MnemonicInfo {
        mnemonic: Zeroizing::new(mnemonic.to_string()),
        word_count,
        entropy_bits,
        checksum_bits: entropy_bits / 32,
        passphrase_used: !passphrase.is_empty(),
        root: root_from_mnemonic(mnemonic, passphrase, network)?,
    })
}

fn root_from_mnemonic(
    mnemonic: &Mnemonic,
    passphrase: &str,
    network: Network,
) -> Result<RootKey, CoreError> {
    let secp = Secp256k1::new();
    let seed = Zeroizing::new(mnemonic.to_seed(passphrase));
    let xprv = Xpriv::new_master(bitcoin::Network::from(network), seed.as_slice())?;
    let xpub = Xpub::from_priv(&secp, &xprv);

    Ok(RootKey {
        network,
        fingerprint: xpub.fingerprint().to_string(),
        xprv: Zeroizing::new(xprv.to_string()),
        xpub: xpub.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// Official BIP39 vectors (https://github.com/trezor/python-mnemonic/blob/master/vectors.json).
    /// They all use the passphrase "TREZOR" and list mainnet `xprv`s.
    const VECTORS: [(&str, &str, &str); 3] = [
        (
            "00000000000000000000000000000000",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "xprv9s21ZrQH143K3h3fDYiay8mocZ3afhfULfb5GX8kCBdno77K4HiA15Tg23wpbeF1pLfs1c5SPmYHrEpTuuRhxMwvKDwqdKiGJS9XFKzUsAF",
        ),
        (
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "xprv9s21ZrQH143K2gA81bYFHqU68xz1cX2APaSq5tt6MFSLeXnCKV1RVUJt9FWNTbrrryem4ZckN8k4Ls1H6nwdvDTvnV7zEXs2HgPezuVccsq",
        ),
        (
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo vote",
            "xprv9s21ZrQH143K2WFF16X85T2QCpndrGwx6GueB72Zf3AHwHJaknRXNF37ZmDrtHrrLSHvbuRejXcnYxoZKvRquTPyp2JiNG3XcjQyzSEgqCB",
        ),
    ];

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Our keys are `tprv`s, the vectors are mainnet `xprv`s: same key material,
    /// different version bytes. Compare what matters.
    fn assert_same_key(ours: &str, expected_mainnet: &str) {
        let ours = Xpriv::from_str(ours).unwrap();
        let expected = Xpriv::from_str(expected_mainnet).unwrap();
        assert_eq!(ours.private_key, expected.private_key);
        assert_eq!(ours.chain_code, expected.chain_code);
    }

    #[test]
    fn matches_official_bip39_vectors() {
        for (entropy, words, xprv) in VECTORS {
            let info = from_entropy(&hex(entropy), "TREZOR", Network::Regtest).unwrap();
            assert_eq!(*info.mnemonic, words);
            assert_same_key(&info.root.xprv, xprv);
        }
    }

    #[test]
    fn parsing_the_words_gives_the_same_root_key() {
        for (_, words, xprv) in VECTORS {
            let root = root_key(words, "TREZOR", Network::Regtest).unwrap();
            assert_same_key(&root.xprv, xprv);
            assert!(root.xprv.starts_with("tprv"));
            assert!(root.xpub.starts_with("tpub"));
        }
    }

    #[test]
    fn entropy_and_checksum_sizes() {
        for words in WORD_COUNTS {
            let info = generate(words, "", Network::Regtest).unwrap();
            assert_eq!(info.word_count, words);
            assert_eq!(info.entropy_bits + info.checksum_bits, words * 11);
        }
    }

    #[test]
    fn passphrase_changes_the_root_key() {
        let words = VECTORS[0].1;
        let without = root_key(words, "", Network::Regtest).unwrap();
        let with = root_key(words, "TREZOR", Network::Regtest).unwrap();
        assert_ne!(without.fingerprint, with.fingerprint);
    }

    #[test]
    fn rejects_bad_checksum() {
        let twelve_abandons = ["abandon"; 12].join(" ");
        assert!(root_key(&twelve_abandons, "", Network::Regtest).is_err());
    }

    #[test]
    fn accepts_uppercase_words() {
        let upper = VECTORS[0].1.to_uppercase();
        assert!(root_key(&upper, "", Network::Regtest).is_ok());
    }
}
