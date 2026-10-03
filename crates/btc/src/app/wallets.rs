//! Named wallets: a label and an address each, kept per network. Nothing secret is stored:
//! a wallet says which coins to look at, and the key is typed in only when signing.

use std::fs;
use std::path::{Path, PathBuf};

use btc_core::Network;
use btc_core::address::AddressType;
use btc_core::derive::{self, DerivedKey, ExtendedKey};
use btc_core::mnemonic;
use btc_core::tx::build::parse_address;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wallet {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    current: Option<String>,
    wallets: Vec<Wallet>,
}

/// The wallets of one network, saved to a file after every change.
#[derive(Debug)]
pub struct Wallets {
    path: PathBuf,
    network: Network,
    saved: Saved,
}

/// `$BTC_HOME/<network>/wallets.json`, else the platform config directory.
pub fn default_path(network: Network) -> Option<PathBuf> {
    let base = match std::env::var_os("BTC_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => directories::ProjectDirs::from("", "", "btc")?
            .config_dir()
            .to_path_buf(),
    };
    Some(base.join(network.as_str()).join("wallets.json"))
}

/// How many words a new recovery phrase has unless the user picks otherwise.
pub const DEFAULT_WORDS: usize = 24;

/// A freshly generated wallet: the phrase to back up, and the address it controls.
pub struct NewWallet {
    pub phrase: Zeroizing<String>,
    pub address: String,
}

/// The first native segwit receiving key of the phrase: `m/84'/<coin>'/0'/0/0`.
fn first_key(network: Network, xprv: &str) -> Result<DerivedKey, AppError> {
    let path = derive::parse_path(&format!(
        "m/84'/{}'/0'/0/0",
        if network.is_mainnet() { 0 } else { 1 }
    ))?;
    let key = ExtendedKey::parse(xprv)?;
    let mut derived = derive::derive(&key, &path, 1, Some(AddressType::P2wpkh), network)?;
    derived
        .children
        .pop()
        .ok_or_else(|| AppError::Input("derivation returned no key".into()))
}

/// A new random recovery phrase (`words` of 12, 15, 18, 21 or 24) and its first address.
/// The phrase is returned, never stored.
pub fn generate(network: Network, words: usize) -> Result<NewWallet, AppError> {
    let info = mnemonic::generate(words, "", network)?;
    let child = first_key(network, &info.root.xprv)?;
    Ok(NewWallet {
        phrase: info.mnemonic,
        address: child.address,
    })
}

/// The private key (WIF) for the first address of a recovery phrase, to sign with.
pub fn wif_from_phrase(network: Network, phrase: &str) -> Result<Zeroizing<String>, AppError> {
    let root = mnemonic::root_key(phrase.trim(), "", network)?;
    first_key(network, &root.xprv)?
        .private_key_wif
        .ok_or_else(|| AppError::Input("derivation returned no private key".into()))
}

/// Replaces `path` with `bytes`: written to a uniquely named file beside it, flushed to disk and
/// renamed over it, so neither a crash nor a second instance leaves half a file. Owner-only on
/// Unix.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

/// Short, lowercase, safe to show and to use as a file name.
pub fn validate_name(name: &str) -> Result<(), AppError> {
    let ok = (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::Input(
            "a wallet name is 1-32 characters: a-z, 0-9, - and _".into(),
        ))
    }
}

impl Wallets {
    /// Reads the file, or starts empty when it does not exist yet.
    pub fn open(path: &Path, network: Network) -> Result<Self, AppError> {
        let saved = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                AppError::Config(format!(
                    "{} is not a valid wallet list: {e}",
                    path.display()
                ))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::default(),
            Err(e) => return Err(AppError::Io(e)),
        };
        Ok(Wallets {
            path: path.to_owned(),
            network,
            saved,
        })
    }

    pub fn list(&self) -> &[Wallet] {
        &self.saved.wallets
    }

    pub fn current(&self) -> Option<&Wallet> {
        let name = self.saved.current.as_deref()?;
        self.saved.wallets.iter().find(|w| w.name == name)
    }

    /// Adds a wallet and makes it the current one. The address must be one `tx sign` can spend
    /// from (native segwit or taproot) on this network.
    pub fn add(&mut self, name: &str, address: &str) -> Result<(), AppError> {
        validate_name(name)?;
        if self.saved.wallets.iter().any(|w| w.name == name) {
            return Err(AppError::Input(format!("a wallet named `{name}` exists")));
        }
        let address = address.trim();
        let parsed = parse_address(address, self.network)?;
        if !parsed.script_pubkey().is_witness_program() {
            return Err(AppError::Input(
                "use a native segwit (bc1q/tb1q/bcrt1q) or taproot (…1p) address: legacy \
                 addresses cannot be signed by this tool"
                    .into(),
            ));
        }
        self.saved.wallets.push(Wallet {
            name: name.to_owned(),
            address: address.to_owned(),
        });
        self.saved.current = Some(name.to_owned());
        self.save()
    }

    pub fn select(&mut self, name: &str) -> Result<(), AppError> {
        if !self.saved.wallets.iter().any(|w| w.name == name) {
            return Err(AppError::Input(format!("no wallet named `{name}`")));
        }
        self.saved.current = Some(name.to_owned());
        self.save()
    }

    pub fn remove(&mut self, name: &str) -> Result<(), AppError> {
        self.saved.wallets.retain(|w| w.name != name);
        if self.saved.current.as_deref() == Some(name) {
            self.saved.current = self.saved.wallets.first().map(|w| w.name.clone());
        }
        self.save()
    }

    fn save(&self) -> Result<(), AppError> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.saved)
            .map_err(|e| AppError::Config(format!("could not save wallets: {e}")))?;
        write_private(&self.path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDR: &str = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
    const LEGACY: &str = "mipcBbFg9gMiCh81Kj8tqqdgoZub1ZJRfn";

    fn store(name: &str) -> (Wallets, PathBuf) {
        let dir = std::env::temp_dir().join(format!("btc-wallets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("wallets.json");
        (Wallets::open(&path, Network::Regtest).unwrap(), dir)
    }

    #[test]
    fn add_select_remove_and_reload() {
        let (mut w, dir) = store("basic");
        assert!(w.current().is_none());
        w.add("alice", ADDR).unwrap();
        w.add("bob", ADDR).unwrap();
        assert_eq!(w.current().unwrap().name, "bob");
        w.select("alice").unwrap();

        let again = Wallets::open(&dir.join("wallets.json"), Network::Regtest).unwrap();
        assert_eq!(again.list().len(), 2);
        assert_eq!(again.current().unwrap().name, "alice");

        w.remove("alice").unwrap();
        assert_eq!(w.current().unwrap().name, "bob");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_generated_wallet_defaults_to_24_words_and_its_phrase_signs_for_its_address() {
        let new = generate(Network::Regtest, DEFAULT_WORDS).unwrap();
        assert_eq!(new.phrase.split_whitespace().count(), 24);
        assert!(new.address.starts_with("bcrt1q"), "{}", new.address);

        // The key derived back from the phrase controls exactly that address.
        let wif = wif_from_phrase(Network::Regtest, &new.phrase).unwrap();
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let secret = btc_core::keys::parse_private_key(&wif).unwrap();
        let public = bitcoin::CompressedPublicKey(secret.inner.public_key(&secp));
        let address = bitcoin::Address::p2wpkh(&public, bitcoin::KnownHrp::Regtest);
        assert_eq!(address.to_string(), new.address);

        for words in [12, 15, 18, 21] {
            let w = generate(Network::Regtest, words).unwrap();
            assert_eq!(w.phrase.split_whitespace().count(), words);
        }
        assert!(generate(Network::Regtest, 13).is_err());
        assert!(wif_from_phrase(Network::Regtest, "not a real phrase at all").is_err());
        // Mainnet uses its own coin type and prefix.
        assert!(
            generate(Network::Mainnet, 24)
                .unwrap()
                .address
                .starts_with("bc1q")
        );
    }

    #[test]
    fn rejects_bad_names_duplicates_and_unsignable_addresses() {
        let (mut w, dir) = store("reject");
        assert!(w.add("Bad Name", ADDR).is_err());
        assert!(w.add("", ADDR).is_err());
        assert!(w.add("legacy", LEGACY).is_err());
        assert!(
            w.add("main", "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu")
                .is_err()
        );
        w.add("ok", ADDR).unwrap();
        assert!(w.add("ok", ADDR).is_err());
        assert!(w.select("nobody").is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
