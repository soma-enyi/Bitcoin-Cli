//! Human-readable rendering for every command result.

use btc_core::address::{AddressReport, KeyFormat, PubkeyAddresses};
use btc_core::derive::{AddressTypeSource, Derivation};
use btc_core::keys::KeyPairInfo;
use btc_core::mnemonic::{MnemonicInfo, RootKey};
use btc_core::tx::decode::{DecodedTx, format_btc};
use comfy_table::{Table, presets};

use super::Render;
use crate::app::config::{AuthView, ConfigView};

/// Aligns `label  value` rows into a column.
fn fields(rows: &[(&str, &str)]) -> String {
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(label, value)| format!("{label:<width$}  {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Numbers the words four per row, the way they are usually written down.
fn numbered_words(mnemonic: &str) -> String {
    let words: Vec<_> = mnemonic.split_whitespace().collect();
    words
        .chunks(4)
        .enumerate()
        .map(|(row, chunk)| {
            chunk
                .iter()
                .enumerate()
                .map(|(i, word)| format!("{:>2}. {word:<10}", row * 4 + i + 1))
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Render for ConfigView {
    fn render_human(&self) -> String {
        let auth = match &self.rpc_auth {
            AuthView::Cookie { path, exists: true } => format!("cookie ({})", path.display()),
            AuthView::Cookie {
                path,
                exists: false,
            } => {
                format!(
                    "cookie ({}, not found: is bitcoind running?)",
                    path.display()
                )
            }
            AuthView::UserPass { user } => format!("user/password (user: {user})"),
            AuthView::ApiKey => "API key (X-API-Key header, value hidden)".to_string(),
        };
        fields(&[
            ("network", self.network.as_str()),
            ("rpc url", &self.rpc_url),
            ("rpc auth", &auth),
        ])
    }
}

impl Render for KeyPairInfo {
    fn render_human(&self) -> String {
        let key_type = match self.parity {
            None => "ecdsa".to_owned(),
            Some(parity) => format!("schnorr (y parity: {parity})"),
        };
        fields(&[
            ("network", self.network.as_str()),
            ("key type", &key_type),
            ("private key (WIF)", &self.private_key_wif),
            ("private key (hex)", &self.private_key_hex),
            ("public key", &self.public_key),
        ])
    }

    fn contains_secrets(&self) -> bool {
        true
    }

    fn is_mainnet(&self) -> bool {
        self.network.is_mainnet()
    }
}

impl Render for MnemonicInfo {
    fn render_human(&self) -> String {
        let size = format!(
            "{} words = {} bits entropy + {} bits checksum",
            self.word_count, self.entropy_bits, self.checksum_bits
        );
        let passphrase = if self.passphrase_used { "yes" } else { "none" };
        format!(
            "{}\n\n{}\n\n{}",
            numbered_words(&self.mnemonic),
            fields(&[("size", &size), ("passphrase", passphrase)]),
            self.root.render_human()
        )
    }

    fn contains_secrets(&self) -> bool {
        true
    }

    fn is_mainnet(&self) -> bool {
        self.root.network.is_mainnet()
    }
}

impl Render for RootKey {
    fn render_human(&self) -> String {
        fields(&[
            ("network", self.network.as_str()),
            ("fingerprint", &self.fingerprint),
            ("root xprv", &self.xprv),
            ("root xpub", &self.xpub),
        ])
    }

    fn contains_secrets(&self) -> bool {
        true
    }

    fn is_mainnet(&self) -> bool {
        self.network.is_mainnet()
    }
}

impl Render for Derivation {
    fn render_human(&self) -> String {
        let input = format!(
            "{} (depth {}, fingerprint {})",
            if self.input_kind == "private" {
                if self.network.is_mainnet() { "xprv" } else { "tprv" }
            } else if self.network.is_mainnet() {
                "xpub"
            } else {
                "tpub"
            },
            self.input_depth,
            self.input_fingerprint
        );
        let source = match self.address_type_source {
            AddressTypeSource::Flag => "--addr-type".to_owned(),
            AddressTypeSource::PathPurpose => format!("BIP{} path", self.address_type.purpose()),
            AddressTypeSource::Default => "default".to_owned(),
        };
        let address_type = format!("{} (from {source})", self.address_type.as_str());
        let header = fields(&[
            ("network", self.network.as_str()),
            ("input", &input),
            ("address type", &address_type),
        ]);

        let body = match self.children.as_slice() {
            [child] => {
                let mut rows = vec![
                    ("path", child.path.as_str()),
                    ("address", &child.address),
                    ("public key", &child.public_key),
                    ("xpub", &child.xpub),
                ];
                if let (Some(xprv), Some(wif)) = (&child.xprv, &child.private_key_wif) {
                    rows.push(("xprv", xprv));
                    rows.push(("private key (WIF)", wif));
                }
                fields(&rows)
            }
            children => {
                let mut table = Table::new();
                table.load_preset(presets::UTF8_FULL_CONDENSED).set_header([
                    "path",
                    "address",
                    "public key",
                ]);
                for child in children {
                    table.add_row([&child.path, &child.address, &child.public_key]);
                }
                format!(
                    "{table}
(use --json to see each child's extended and private keys)"
                )
            }
        };

        let notes: String = self
            .warnings
            .iter()
            .map(|w| format!("\nnote: {w}"))
            .collect();
        format!("{header}\n\n{body}{notes}")
    }

    fn contains_secrets(&self) -> bool {
        Derivation::contains_secrets(self)
    }

    fn is_mainnet(&self) -> bool {
        self.network.is_mainnet()
    }
}

impl Render for PubkeyAddresses {
    fn render_human(&self) -> String {
        let format = match self.key_format {
            KeyFormat::Compressed => "compressed",
            KeyFormat::Uncompressed => "uncompressed",
            KeyFormat::XOnly => "x-only",
        };
        let header = fields(&[
            ("network", self.network.as_str()),
            ("public key", &self.public_key),
            ("key format", format),
        ]);

        let mut table = Table::new();
        table
            .load_preset(presets::UTF8_FULL_CONDENSED)
            .set_header(["type", "address"]);
        for a in &self.addresses {
            table.add_row([a.address_type.as_str(), &a.address]);
        }

        let notes: String = self
            .unsupported
            .iter()
            .map(|u| format!("\nnote: no {}: {}", u.address_type.as_str(), u.reason))
            .collect();
        format!("{header}\n\n{table}{notes}")
    }
}

impl Render for AddressReport {
    fn render_human(&self) -> String {
        let witness_version = self.witness_version.map(|v| v.to_string());
        let valid_on = self.valid_networks.join(", ");
        let mut rows = vec![
            ("address", self.address.as_str()),
            ("valid", "yes"),
            ("type", &self.address_type),
            ("encoding", self.encoding),
        ];
        if let Some(v) = &witness_version {
            rows.push(("witness version", v));
        }
        rows.extend([
            ("payload", self.payload.as_str()),
            ("valid on", &valid_on),
            ("scriptPubKey", &self.script_pubkey),
            ("scriptPubKey asm", &self.script_pubkey_asm),
        ]);

        let notes: String = self.notes.iter().map(|n| format!("\nnote: {n}")).collect();
        format!(
            "{}{}",
            fields(&rows),
            if notes.is_empty() {
                notes
            } else {
                format!("\n{notes}")
            }
        )
    }
}

/// Long hex (signatures, scripts) shortened to fit a line; `--json` has it in full.
fn short_hex(hex: &str) -> String {
    if hex.len() <= 24 {
        return hex.to_owned();
    }
    format!(
        "{}…{} ({} bytes)",
        &hex[..12],
        &hex[hex.len() - 8..],
        hex.len() / 2
    )
}

/// `txid:vout` with the txid shortened: `d95ea5400c61…fe04d769:3`.
fn short_outpoint(outpoint: &str) -> String {
    match outpoint.split_once(':') {
        Some((txid, vout)) if txid.len() > 20 => {
            format!("{}…{}:{vout}", &txid[..12], &txid[txid.len() - 8..])
        }
        _ => outpoint.to_owned(),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

impl Render for DecodedTx {
    fn render_human(&self) -> String {
        let flags = format!(
            "{}   segwit: {}   rbf: {}   coinbase: {}",
            self.version,
            yes_no(self.segwit),
            yes_no(self.rbf),
            yes_no(self.coinbase)
        );
        let locktime = format!(
            "{} ({}{})",
            self.locktime.value,
            self.locktime.meaning,
            if self.locktime.enforced {
                ", enforced"
            } else {
                ""
            }
        );
        let size = format!(
            "{} bytes (base {}) · weight {} WU · vsize {} vB",
            self.size, self.base_size, self.weight, self.vsize
        );
        let fee = match &self.fee {
            Some(fee) => format!("{} sats ({} sat/vB)", fee.sats, fee.sat_per_vb),
            None => "unknown".to_owned(),
        };
        let total_out = format!("{} BTC", format_btc(self.total_output_sats));
        let header = fields(&[
            ("txid", &self.txid),
            ("wtxid", &self.wtxid),
            ("version", &flags),
            ("locktime", &locktime),
            ("size", &size),
            ("total out", &total_out),
            ("fee", &fee),
        ]);

        let mut inputs = Table::new();
        inputs
            .load_preset(presets::UTF8_FULL_CONDENSED)
            .set_header([
                "#",
                "previous output",
                "spend type",
                "sequence",
                "value (BTC)",
            ]);
        for input in &self.inputs {
            inputs.add_row([
                input.index.to_string(),
                input
                    .previous_output
                    .as_deref()
                    .map(short_outpoint)
                    .unwrap_or_else(|| "coinbase".to_owned()),
                input.spend_type.clone(),
                format!("{:#010x}\n{}", input.sequence, input.sequence_meaning),
                input
                    .value_sats
                    .map(format_btc)
                    .unwrap_or_else(|| "?".to_owned()),
            ]);
        }

        let mut details = Vec::new();
        for input in &self.inputs {
            if let Some(height) = input.coinbase_height {
                details.push(format!(
                    "input {}: block height {height} (BIP34)",
                    input.index
                ));
            }
            if !input.script_sig.asm.is_empty() && input.coinbase_height.is_none() {
                details.push(format!(
                    "input {} scriptSig: {}",
                    input.index,
                    input
                        .script_sig
                        .asm
                        .split(' ')
                        .map(short_hex)
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            if !input.witness.is_empty() {
                let items: Vec<_> = input.witness.iter().map(|w| short_hex(w)).collect();
                details.push(format!(
                    "input {} witness: [{}]",
                    input.index,
                    items.join(", ")
                ));
            }
        }

        let mut outputs = Table::new();
        outputs
            .load_preset(presets::UTF8_FULL_CONDENSED)
            .set_header(["#", "value (BTC)", "type", "address / data"]);
        for output in &self.outputs {
            let destination = match (&output.address, &output.op_return) {
                (Some(address), _) => address.clone(),
                (None, Some(data)) => match (data.purpose, &data.text) {
                    (Some(purpose), _) => purpose.to_owned(),
                    (None, Some(text)) => format!("\"{text}\""),
                    (None, None) => short_hex(&data.hex),
                },
                (None, None) => short_hex(&output.script_pubkey.hex),
            };
            outputs.add_row([
                output.index.to_string(),
                output.value_btc.clone(),
                output.script_type.as_str().to_owned(),
                destination,
            ]);
        }

        let notes: String = self.notes.iter().map(|n| format!("\nnote: {n}")).collect();
        format!(
            "{header}\n\ninputs ({})\n{inputs}\n{}\n\noutputs ({})\n{outputs}\n{notes}",
            self.inputs.len(),
            details.join("\n"),
            self.outputs.len()
        )
    }
}
