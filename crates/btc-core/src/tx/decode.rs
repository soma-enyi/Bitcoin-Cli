//! Decoding a raw transaction field by field.
//!
//! Serialization (BIP144 for SegWit):
//!   version | [marker 0x00, flag 0x01] | inputs | outputs | [witnesses] | locktime
//! The txid hashes everything except the marker, flag and witnesses, so signatures
//! can't change it (no malleability). The wtxid hashes everything.

use bitcoin::absolute::LockTime;
use bitcoin::consensus::encode;
use bitcoin::hex::DisplayHex;
use bitcoin::script::Instruction;
use bitcoin::{Address, Amount, Script, Sequence, Transaction, TxIn, TxOut, relative};
use serde::Serialize;

use crate::script::{self, ScriptType, SpendTypeSource};
use crate::{CoreError, Network};

#[derive(Serialize)]
pub struct DecodedTx {
    pub network: Network,
    pub txid: String,
    pub wtxid: String,
    pub version: i32,
    pub segwit: bool,
    pub coinbase: bool,
    /// BIP125: any input sequence below 0xfffffffe lets the sender replace this
    /// transaction with a higher-fee version while it is unconfirmed.
    pub rbf: bool,
    pub locktime: LocktimeView,
    /// Bytes on the wire, including witness data.
    pub size: usize,
    /// Bytes without witness data (what the txid covers).
    pub base_size: usize,
    /// base_size × 4 + witness bytes × 1. Blocks hold at most 4,000,000 weight units.
    pub weight: u64,
    /// weight / 4, rounded up. Fee rates are quoted per vbyte.
    pub vsize: u64,
    pub inputs: Vec<DecodedInput>,
    pub outputs: Vec<DecodedOutput>,
    pub total_output_sats: u64,
    /// Only known when the previous outputs are supplied (a raw tx doesn't contain them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_input_sats: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<Fee>,
    pub notes: Vec<String>,
}

#[derive(Serialize)]
pub struct LocktimeView {
    pub value: u32,
    /// "none", "block height N" or "unix time N".
    pub meaning: String,
    /// Locktime only applies if at least one input's sequence is below 0xffffffff.
    pub enforced: bool,
}

#[derive(Serialize)]
pub struct Fee {
    pub sats: u64,
    pub sat_per_vb: f64,
}

#[derive(Serialize)]
pub struct ScriptView {
    pub hex: String,
    pub asm: String,
}

#[derive(Serialize)]
pub struct DecodedInput {
    pub index: usize,
    /// `txid:vout` of the output being spent; absent for a coinbase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<String>,
    pub spend_type: String,
    pub spend_type_source: SpendTypeSource,
    pub script_sig: ScriptView,
    pub witness: Vec<String>,
    pub sequence: u32,
    pub sequence_meaning: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_sats: Option<u64>,
    /// BIP34: a coinbase's scriptSig starts with the block height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coinbase_height: Option<i64>,
}

#[derive(Serialize)]
pub struct DecodedOutput {
    pub index: usize,
    pub value_sats: u64,
    pub value_btc: String,
    pub script_type: ScriptType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub script_pubkey: ScriptView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_return: Option<OpReturnData>,
}

#[derive(Serialize)]
pub struct OpReturnData {
    pub hex: String,
    /// The data as text, when it is printable UTF-8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// e.g. "segwit witness commitment".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<&'static str>,
}

/// Parses raw transaction hex. Whitespace (e.g. line breaks from copy-paste) is ignored.
pub fn parse_hex(input: &str) -> Result<Transaction, CoreError> {
    let hex: String = input.chars().filter(|c| !c.is_whitespace()).collect();

    if hex.starts_with("cHNidP") {
        return Err(CoreError::InvalidInput(
            "this is a base64 PSBT, not raw transaction hex".into(),
        ));
    }
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidInput(
            "this looks like a txid; decoding needs the raw transaction hex \
             (bitcoin-cli getrawtransaction <txid>)"
                .into(),
        ));
    }
    encode::deserialize_hex(&hex)
        .map_err(|e| CoreError::InvalidInput(format!("not a valid raw transaction: {e}")))
}

/// Decodes `tx`. `prevouts` (one per input, in order) enables input values and the fee.
pub fn decode(
    tx: &Transaction,
    network: Network,
    prevouts: Option<&[TxOut]>,
) -> Result<DecodedTx, CoreError> {
    if let Some(prevouts) = prevouts {
        if prevouts.len() != tx.input.len() {
            return Err(CoreError::InvalidInput(format!(
                "{} previous outputs given for {} inputs",
                prevouts.len(),
                tx.input.len()
            )));
        }
    }

    let coinbase = tx.is_coinbase();
    let segwit = tx.input.iter().any(|i| !i.witness.is_empty());
    let inputs: Vec<_> = tx
        .input
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let prevout = prevouts.map(|p| &p[index]);
            decode_input(index, input, prevout, tx.version.0, coinbase)
        })
        .collect();
    let outputs: Vec<_> = tx
        .output
        .iter()
        .enumerate()
        .map(|(index, output)| decode_output(index, output, network))
        .collect();

    let total_output = tx.output.iter().map(|o| o.value).sum::<Amount>().to_sat();
    let total_input = match prevouts {
        Some(prevouts) if !coinbase => {
            Some(prevouts.iter().map(|p| p.value).sum::<Amount>().to_sat())
        }
        _ => None,
    };
    let vsize = tx.vsize() as u64;
    let fee = match total_input {
        Some(total_input) => {
            let sats = total_input.checked_sub(total_output).ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "outputs ({total_output} sats) exceed inputs ({total_input} sats)"
                ))
            })?;
            Some(Fee {
                sats,
                sat_per_vb: (sats as f64 / vsize as f64 * 100.0).round() / 100.0,
            })
        }
        None => None,
    };

    let size = tx.total_size();
    let base_size = tx.base_size();
    let mut notes = Vec::new();
    if coinbase {
        notes.push(
            "coinbase: creates the block subsidy plus fees out of nothing, so it has no real \
             input and no fee"
                .to_owned(),
        );
    } else if fee.is_none() {
        notes.push(
            "fee unknown: a raw transaction doesn't contain its input amounts; they live in \
             the previous transactions (decode it as <txid>@<block height>, or use --prevouts-from-node, to fetch them from a node)"
                .to_owned(),
        );
    }
    if segwit {
        notes.push(format!(
            "segwit: {} of {size} bytes are witness data, counted at 1/4 weight",
            size - base_size
        ));
    }

    Ok(DecodedTx {
        network,
        txid: tx.compute_txid().to_string(),
        wtxid: tx.compute_wtxid().to_string(),
        version: tx.version.0,
        segwit,
        coinbase,
        rbf: tx.is_explicitly_rbf(),
        locktime: locktime(tx),
        size,
        base_size,
        weight: tx.weight().to_wu(),
        vsize,
        inputs,
        outputs,
        total_output_sats: total_output,
        total_input_sats: total_input,
        fee,
        notes,
    })
}

fn decode_input(
    index: usize,
    input: &TxIn,
    prevout: Option<&TxOut>,
    tx_version: i32,
    coinbase: bool,
) -> DecodedInput {
    let (spend_type, spend_type_source) = if coinbase {
        ("coinbase".to_owned(), SpendTypeSource::Inferred)
    } else {
        script::spend_type(input, prevout)
    };

    DecodedInput {
        index,
        previous_output: (!coinbase).then(|| input.previous_output.to_string()),
        spend_type,
        spend_type_source,
        script_sig: script_view(&input.script_sig),
        witness: input
            .witness
            .iter()
            .map(|w| w.to_lower_hex_string())
            .collect(),
        sequence: input.sequence.0,
        sequence_meaning: sequence_meaning(input.sequence, tx_version),
        value_sats: prevout.filter(|_| !coinbase).map(|p| p.value.to_sat()),
        coinbase_height: coinbase.then(|| bip34_height(&input.script_sig)).flatten(),
    }
}

fn decode_output(index: usize, output: &TxOut, network: Network) -> DecodedOutput {
    let script = &output.script_pubkey;
    let script_type = script::classify(script);
    let op_return = (script_type == ScriptType::OpReturn).then(|| {
        let data = script::op_return_data(script);
        OpReturnData {
            hex: data.to_lower_hex_string(),
            text: String::from_utf8(data)
                .ok()
                .filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control())),
            purpose: script::is_witness_commitment(script).then_some("segwit witness commitment"),
        }
    });

    DecodedOutput {
        index,
        value_sats: output.value.to_sat(),
        value_btc: format_btc(output.value.to_sat()),
        script_type,
        address: Address::from_script(script, bitcoin::Network::from(network))
            .ok()
            .map(|a| a.to_string()),
        script_pubkey: script_view(script),
        op_return,
    }
}

fn locktime(tx: &Transaction) -> LocktimeView {
    let value = tx.lock_time.to_consensus_u32();
    let meaning = match tx.lock_time {
        _ if value == 0 => "none".to_owned(),
        LockTime::Blocks(height) => format!("block height {height}"),
        LockTime::Seconds(time) => format!("unix time {time}"),
    };
    LocktimeView {
        value,
        meaning,
        enforced: value != 0 && tx.is_lock_time_enabled(),
    }
}

fn sequence_meaning(sequence: Sequence, tx_version: i32) -> String {
    let mut meaning = match sequence {
        Sequence::MAX => "final: no RBF, locktime disabled".to_owned(),
        Sequence::ENABLE_LOCKTIME_NO_RBF => "no RBF, locktime enabled".to_owned(),
        _ => "signals RBF".to_owned(),
    };
    // BIP68 relative locktimes only apply to version 2+ transactions.
    if tx_version >= 2 {
        match sequence.to_relative_lock_time() {
            Some(relative::LockTime::Blocks(height)) => {
                meaning.push_str(&format!(", relative locktime {} blocks", height.value()));
            }
            Some(relative::LockTime::Time(time)) => {
                meaning.push_str(&format!(
                    ", relative locktime {} seconds",
                    u32::from(time.value()) * 512
                ));
            }
            None => {}
        }
    }
    meaning
}

fn bip34_height(script_sig: &Script) -> Option<i64> {
    match script_sig.instructions().next()?.ok()? {
        Instruction::PushBytes(bytes) => {
            bitcoin::script::read_scriptint_non_minimal(bytes.as_bytes()).ok()
        }
        Instruction::Op(op) => match op.to_u8() {
            0x00 => Some(0),
            n @ 0x51..=0x60 => Some(i64::from(n - 0x50)),
            _ => None,
        },
    }
}

fn script_view(script: &Script) -> ScriptView {
    ScriptView {
        hex: script.to_hex_string(),
        asm: script.to_asm_string(),
    }
}

pub fn format_btc(sats: u64) -> String {
    format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000)
}

#[cfg(test)]
mod tests {
    use bitcoin::ScriptBuf;
    use serde_json::Value;

    use super::*;

    /// Real transactions built on a local Bitcoin Core v30 regtest node, with Core's own
    /// `getrawtransaction <txid> 2` results as the expected values.
    fn fixtures() -> Value {
        serde_json::from_str(include_str!("../../testdata/regtest_txs.json")).unwrap()
    }

    fn decode_fixture(name: &str, with_prevouts: bool) -> (DecodedTx, Value) {
        let fixture = fixtures()[name].clone();
        let tx = parse_hex(fixture["hex"].as_str().unwrap()).unwrap();
        let prevouts: Option<Vec<TxOut>> = with_prevouts.then(|| {
            fixture["vin"]
                .as_array()
                .unwrap()
                .iter()
                .map(|vin| TxOut {
                    value: Amount::from_sat(vin["prevout"]["value_sats"].as_u64().unwrap()),
                    script_pubkey: ScriptBuf::from_hex(
                        vin["prevout"]["script_pubkey"].as_str().unwrap(),
                    )
                    .unwrap(),
                })
                .collect()
        });
        let decoded = decode(&tx, Network::Regtest, prevouts.as_deref()).unwrap();
        (decoded, fixture)
    }

    /// Bitcoin Core's names for output types.
    fn core_type(t: ScriptType) -> &'static str {
        match t {
            ScriptType::P2pkh => "pubkeyhash",
            ScriptType::P2sh => "scripthash",
            ScriptType::P2wpkh => "witness_v0_keyhash",
            ScriptType::P2wsh => "witness_v0_scripthash",
            ScriptType::P2tr => "witness_v1_taproot",
            ScriptType::OpReturn => "nulldata",
            _ => "other",
        }
    }

    #[test]
    fn matches_bitcoin_core_for_every_fixture() {
        for name in ["funding", "mixed_inputs", "coinbase"] {
            let (tx, expected) = decode_fixture(name, false);
            assert_eq!(tx.txid, expected["txid"], "{name} txid");
            assert_eq!(tx.wtxid, expected["hash"], "{name} wtxid");
            assert_eq!(tx.size as u64, expected["size"], "{name} size");
            assert_eq!(tx.vsize, expected["vsize"], "{name} vsize");
            assert_eq!(tx.weight, expected["weight"], "{name} weight");
            assert_eq!(tx.version as i64, expected["version"], "{name} version");
            assert_eq!(
                tx.locktime.value as u64, expected["locktime"],
                "{name} locktime"
            );

            let expected_outputs = expected["vout"].as_array().unwrap();
            assert_eq!(tx.outputs.len(), expected_outputs.len());
            for (ours, core) in tx.outputs.iter().zip(expected_outputs) {
                assert_eq!(ours.value_sats, core["value_sats"], "{name} value");
                assert_eq!(core_type(ours.script_type), core["type"], "{name} type");
                assert_eq!(
                    ours.address.as_deref(),
                    core["address"].as_str(),
                    "{name} address"
                );
            }
            for (ours, core) in tx.inputs.iter().zip(expected["vin"].as_array().unwrap()) {
                assert_eq!(ours.sequence as u64, core["sequence"], "{name} sequence");
            }
        }
    }

    #[test]
    fn infers_every_input_type_without_prevouts() {
        let (tx, _) = decode_fixture("mixed_inputs", false);
        let spend_types: Vec<_> = tx.inputs.iter().map(|i| i.spend_type.as_str()).collect();
        assert_eq!(
            spend_types,
            ["p2pkh", "p2sh-p2wpkh", "p2wpkh", "p2tr_key_path"]
        );
        assert!(
            tx.inputs
                .iter()
                .all(|i| i.spend_type_source == SpendTypeSource::Inferred)
        );
        assert!(tx.fee.is_none());
        assert!(tx.segwit);
        assert!(tx.rbf);
    }

    #[test]
    fn prevouts_give_the_same_fee_as_bitcoin_core() {
        let (tx, expected) = decode_fixture("mixed_inputs", true);
        let fee = tx.fee.unwrap();
        assert_eq!(fee.sats, expected["fee_sats"]);
        assert_eq!(
            fee.sat_per_vb,
            (fee.sats as f64 / tx.vsize as f64 * 100.0).round() / 100.0
        );
        assert_eq!(
            tx.total_input_sats.unwrap(),
            tx.total_output_sats + fee.sats
        );
        let spend_types: Vec<_> = tx.inputs.iter().map(|i| i.spend_type.as_str()).collect();
        assert_eq!(
            spend_types,
            ["p2pkh", "p2sh-p2wpkh", "p2wpkh", "p2tr_key_path"]
        );
        assert!(
            tx.inputs
                .iter()
                .all(|i| i.spend_type_source == SpendTypeSource::Prevout)
        );
    }

    #[test]
    fn decodes_op_return_text() {
        let (tx, _) = decode_fixture("mixed_inputs", false);
        let data = tx
            .outputs
            .iter()
            .find_map(|o| o.op_return.as_ref())
            .unwrap();
        assert_eq!(data.text.as_deref(), Some("hello btc-cli"));
    }

    #[test]
    fn coinbase_height_and_witness_commitment() {
        let (tx, expected) = decode_fixture("coinbase", false);
        let fixtures = fixtures();
        assert!(tx.coinbase);
        assert!(tx.fee.is_none());
        assert_eq!(tx.inputs[0].spend_type, "coinbase");
        assert_eq!(
            tx.inputs[0].coinbase_height,
            fixtures["coinbase_height"].as_i64()
        );
        assert!(tx.inputs[0].previous_output.is_none());
        let commitment = tx.outputs[1].op_return.as_ref().unwrap();
        assert_eq!(commitment.purpose, Some("segwit witness commitment"));
        assert_eq!(expected["vout"][1]["type"], "nulldata");
    }

    #[test]
    fn sequence_meanings() {
        assert_eq!(
            sequence_meaning(Sequence::MAX, 2),
            "final: no RBF, locktime disabled"
        );
        assert_eq!(
            sequence_meaning(Sequence::ENABLE_RBF_NO_LOCKTIME, 2),
            "signals RBF"
        );
        assert_eq!(
            sequence_meaning(Sequence::from_height(144), 2),
            "signals RBF, relative locktime 144 blocks"
        );
        // Relative locktimes don't exist in version 1 transactions.
        assert_eq!(
            sequence_meaning(Sequence::from_height(144), 1),
            "signals RBF"
        );
    }

    #[test]
    fn rejects_bad_input_with_hints() {
        assert!(parse_hex("zz").is_err());
        let fixture = fixtures()["funding"]["hex"].as_str().unwrap().to_owned();
        assert!(
            parse_hex(&format!("{fixture}00")).is_err(),
            "trailing bytes"
        );
        let txid = "b94b83b802022270f19785ff8d5cf0a92754df1c384be693a077a27902534acf";
        assert!(parse_hex(txid).err().unwrap().to_string().contains("txid"));
        assert!(
            parse_hex("cHNidP8BAHECAAAAAQ==")
                .err()
                .unwrap()
                .to_string()
                .contains("PSBT")
        );
        // Line breaks from copy-paste are fine.
        let wrapped = fixture
            .as_bytes()
            .chunks(64)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_hex(&wrapped).is_ok());
    }

    #[test]
    fn prevout_count_must_match_inputs() {
        let tx = parse_hex(fixtures()["funding"]["hex"].as_str().unwrap()).unwrap();
        let err = decode(&tx, Network::Regtest, Some(&[])).err().unwrap();
        assert!(err.to_string().contains("previous outputs"));
    }
}
