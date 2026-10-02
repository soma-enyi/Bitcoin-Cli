use std::str::FromStr;

use bitcoin::psbt::Psbt;
use bitcoin::{Amount, ScriptBuf, Transaction, TxOut};
use serde::Serialize;
use zeroize::Zeroizing;

use btc_core::keys::parse_private_key;
use btc_core::tx::build::{TxInput, TxOutput, create_psbt, parse_address};
use btc_core::tx::decode::{self, DecodedTx};
use btc_core::tx::sign::{extract_signed, sign_psbt};

use super::input::{ensure_single_stdin, secret_arg, text_arg};
use super::{AppError, Context};
use crate::output::Render;

#[derive(Serialize)]
pub struct PsbtOutput {
    pub psbt: String,
}

impl Render for PsbtOutput {
    fn render_human(&self) -> String {
        self.psbt.clone()
    }
}

#[derive(Serialize)]
pub struct TxHexOutput {
    pub hex: String,
    pub txid: String,
}

impl Render for TxHexOutput {
    fn render_human(&self) -> String {
        format!("hex:  {}\ntxid: {}", self.hex, self.txid)
    }

    fn contains_secrets(&self) -> bool {
        false
    }
}

#[derive(Serialize)]
pub struct BroadcastOutput {
    pub txid: String,
}

impl Render for BroadcastOutput {
    fn render_human(&self) -> String {
        self.txid.clone()
    }
}

/// What the user typed into `tx decode`.
enum TxInputKind {
    /// Raw transaction hex (or anything else: the parser reports what is wrong with it).
    Hex,
    /// `<txid>` or `<txid>@<height or block hash>`: fetch the hex from the node.
    Txid { txid: String, block: Option<String> },
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn classify(input: &str) -> Result<TxInputKind, AppError> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if is_hex64(&compact) {
        return Ok(TxInputKind::Txid {
            txid: compact,
            block: None,
        });
    }
    if let Some((txid, block)) = compact.split_once('@') {
        if !is_hex64(txid) {
            return Err(AppError::Input(
                "before the @ there must be a 64-character txid: <txid>@<block height or hash>"
                    .into(),
            ));
        }
        let height = !block.is_empty() && block.chars().all(|c| c.is_ascii_digit());
        if !height && !is_hex64(block) {
            return Err(AppError::Input(
                "after the @ there must be a block height or a 64-character block hash".into(),
            ));
        }
        return Ok(TxInputKind::Txid {
            txid: txid.to_owned(),
            block: Some(block.to_owned()),
        });
    }
    Ok(TxInputKind::Hex)
}

/// Whether to ask the node for the amounts the inputs spend, which give the fee.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrevoutMode {
    /// Offline: the fee stays unknown.
    Off,
    /// Ask the node, and fail if it cannot say (`--prevouts-from-node`).
    Required,
    /// Ask the node, and keep going without a fee if it cannot (the TUI).
    BestEffort,
}

const NO_INDEX_HINT: &str = "this node has no transaction index, so it cannot look up the amounts \
    this transaction spends. Decode it as <txid>@<block height> (the block is where it confirmed)";

fn to_txouts(prevouts: &[(u64, String)]) -> Result<Vec<TxOut>, AppError> {
    prevouts
        .iter()
        .map(|(sats, script_hex)| {
            let script = ScriptBuf::from_hex(script_hex)
                .map_err(|e| AppError::Input(format!("node returned a bad script: {e}")))?;
            Ok(TxOut {
                value: Amount::from_sat(*sats),
                script_pubkey: script,
            })
        })
        .collect()
}

/// The outputs `tx` spends, from the node: in one call when it supports verbosity 2,
/// otherwise by fetching each previous transaction (needs a transaction index).
fn prevouts_from_node(ctx: &Context, tx: &Transaction) -> Result<Vec<TxOut>, AppError> {
    super::node::ensure_chain(ctx)?;
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;

    let own = backend
        .transaction(&tx.compute_txid().to_string(), None)
        .map_err(|e| match &e {
            btc_node::NodeError::Rpc(msg) if msg.contains("No such mempool transaction") => {
                AppError::Input(NO_INDEX_HINT.into())
            }
            _ => AppError::Node(e),
        })?;
    if let Some(prevouts) = own.prevouts {
        return to_txouts(&prevouts);
    }

    let mut outs = Vec::new();
    for input in &tx.input {
        let prev = input.previous_output;
        let hex = backend
            .raw_transaction(&prev.txid.to_string(), None)
            .map_err(|_| AppError::Input(NO_INDEX_HINT.into()))?;
        let prev_tx = decode::parse_hex(&hex)?;
        let out = prev_tx.output.get(prev.vout as usize).ok_or_else(|| {
            AppError::Input(format!("{}:{} does not exist", prev.txid, prev.vout))
        })?;
        outs.push(out.clone());
    }
    Ok(outs)
}

/// Decodes raw hex, or fetches the transaction from the node first when given a txid.
pub fn decode(ctx: &Context, input: &str, mode: PrevoutMode) -> Result<DecodedTx, AppError> {
    let input = text_arg(input)?;
    let (hex, mut prevouts) = match classify(&input)? {
        TxInputKind::Hex => (input, None),
        TxInputKind::Txid { txid, block } => {
            super::node::ensure_chain(ctx)?;
            let backend = ctx
                .backend
                .as_ref()
                .ok_or(AppError::NotImplemented("node backend"))?;
            let fetched = backend.transaction(&txid, block.as_deref()).map_err(|e| match (&e, &block) {
                (btc_node::NodeError::Rpc(msg), None) if msg.contains("No such mempool transaction") => {
                    AppError::Input(
                        "this node has no transaction index, so by txid alone it can only find \
                         unconfirmed transactions. For a confirmed one add its block: \
                         <txid>@<height or block hash> (a block explorer shows the height)"
                            .into(),
                    )
                }
                _ => AppError::Node(e),
            })?;
            let prevouts = fetched.prevouts.as_deref().map(to_txouts).transpose()?;
            (fetched.hex, prevouts)
        }
    };
    let tx = decode::parse_hex(&hex)?;

    let mut failure = None;
    if prevouts.is_none() && mode != PrevoutMode::Off && !tx.is_coinbase() {
        match prevouts_from_node(ctx, &tx) {
            Ok(found) => prevouts = Some(found),
            Err(e) if mode == PrevoutMode::Required => return Err(e),
            Err(e) => failure = Some(e.to_string()),
        }
    }
    if tx.is_coinbase() {
        prevouts = None;
    }

    let mut decoded = decode::decode(&tx, ctx.network, prevouts.as_deref())?;
    if let Some(reason) = failure {
        decoded.notes.retain(|n| !n.starts_with("fee unknown"));
        decoded.notes.push(format!("fee unknown: {reason}"));
    }
    Ok(decoded)
}

pub fn create(
    _ctx: &Context,
    inputs: &[String],
    outputs: &[String],
    change: Option<&str>,
    fee_rate: f64,
) -> Result<PsbtOutput, AppError> {
    let mut tx_inputs = Vec::new();
    for input_str in inputs {
        tx_inputs.push(TxInput::parse(input_str, _ctx.network)?);
    }

    let mut tx_outputs = Vec::new();
    for output_str in outputs {
        tx_outputs.push(TxOutput::parse(output_str, _ctx.network)?);
    }

    let change_address = if let Some(addr) = change {
        parse_address(addr, _ctx.network)
            .map_err(|e| AppError::Input(format!("invalid change address: {e}")))?
    } else {
        return Err(AppError::Input("change address required".into()));
    };

    if fee_rate <= 0.0 {
        return Err(AppError::Input("fee rate must be positive".into()));
    }

    let psbt = create_psbt(tx_inputs, tx_outputs, Some(change_address), fee_rate)?;
    Ok(PsbtOutput {
        psbt: psbt.to_string(),
    })
}

pub fn sign(
    ctx: &Context,
    psbt_str: &str,
    key: Option<&str>,
    key_file: Option<&str>,
) -> Result<TxHexOutput, AppError> {
    ensure_single_stdin(&[Some(psbt_str), key, key_file])?;

    let psbt_text = text_arg(psbt_str)?;
    let mut psbt = Psbt::from_str(psbt_text.trim())
        .map_err(|e| AppError::Input(format!("invalid PSBT: {e}")))?;

    let key_source = if let Some(k) = key {
        secret_arg(k)?
    } else if let Some(kf) = key_file {
        let content = std::fs::read_to_string(kf).map_err(AppError::Io)?;
        Zeroizing::new(content.trim().to_owned())
    } else {
        return Err(AppError::Input("no key provided".into()));
    };

    let private_key = parse_private_key(&key_source)?;
    if private_key.network.is_mainnet() != ctx.network.is_mainnet() {
        return Err(AppError::Input(format!(
            "this is a {} private key but the network is {}",
            if private_key.network.is_mainnet() {
                "mainnet"
            } else {
                "test-network"
            },
            ctx.network
        )));
    }

    sign_psbt(&mut psbt, &private_key.inner)?;
    let tx = extract_signed(psbt)?;

    Ok(TxHexOutput {
        hex: bitcoin::consensus::encode::serialize_hex(&tx),
        txid: tx.compute_txid().to_string(),
    })
}

pub fn broadcast(ctx: &Context, hex: &str, yes: bool) -> Result<BroadcastOutput, AppError> {
    let tx_hex = text_arg(hex)?;
    let tx_hex = tx_hex.trim();
    super::validate::transaction_hex(tx_hex)?;
    decode::parse_hex(tx_hex)?;

    if ctx.network.is_mainnet() && !yes {
        return Err(AppError::Input(
            "this broadcasts to MAINNET: it spends real bitcoin and cannot be undone. \
             Check the transaction with `btc tx decode`, then re-run with --yes"
                .into(),
        ));
    }
    super::node::ensure_chain(ctx)?;

    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;
    Ok(BroadcastOutput {
        txid: backend.send_raw_transaction(tx_hex)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TXID: &str = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b";

    #[test]
    fn classifies_what_the_user_typed() {
        assert!(matches!(classify("0100000001").unwrap(), TxInputKind::Hex));
        assert!(matches!(
            classify(TXID).unwrap(),
            TxInputKind::Txid { block: None, .. }
        ));
        assert!(matches!(
            classify(&format!("{TXID}@840000")).unwrap(),
            TxInputKind::Txid { block: Some(b), .. } if b == "840000"
        ));
        assert!(matches!(
            classify(&format!("  {TXID}@{TXID}  ")).unwrap(),
            TxInputKind::Txid { block: Some(_), .. }
        ));
    }

    #[test]
    fn rejects_malformed_txid_forms() {
        assert!(classify("abc@1").is_err());
        assert!(classify(&format!("{TXID}@")).is_err());
        assert!(classify(&format!("{TXID}@12ab")).is_err());
    }
}
