//! Classifying scripts: what an output locks to, and how an input spends.

use bitcoin::script::{Instruction, Script};
use bitcoin::{TxIn, TxOut};
use serde::Serialize;

/// The standard output script templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    /// Pay to public key: the earliest form, used by early coinbase outputs.
    P2pk,
    P2pkh,
    P2sh,
    P2wpkh,
    P2wsh,
    P2tr,
    /// Pay to anchor (`OP_1 <4e73>`): a keyless output for fee bumping.
    P2a,
    /// Bare `m-of-n OP_CHECKMULTISIG`.
    Multisig,
    /// Provably unspendable data carrier.
    OpReturn,
    /// A witness version not yet defined by a soft fork.
    WitnessUnknown,
    Nonstandard,
}

impl ScriptType {
    pub fn as_str(self) -> &'static str {
        match self {
            ScriptType::P2pk => "p2pk",
            ScriptType::P2pkh => "p2pkh",
            ScriptType::P2sh => "p2sh",
            ScriptType::P2wpkh => "p2wpkh",
            ScriptType::P2wsh => "p2wsh",
            ScriptType::P2tr => "p2tr",
            ScriptType::P2a => "p2a",
            ScriptType::Multisig => "multisig",
            ScriptType::OpReturn => "op_return",
            ScriptType::WitnessUnknown => "witness_unknown",
            ScriptType::Nonstandard => "nonstandard",
        }
    }
}

pub fn classify(script: &Script) -> ScriptType {
    if script.is_p2pkh() {
        ScriptType::P2pkh
    } else if script.is_p2sh() {
        ScriptType::P2sh
    } else if script.is_p2wpkh() {
        ScriptType::P2wpkh
    } else if script.is_p2wsh() {
        ScriptType::P2wsh
    } else if script.is_p2tr() {
        ScriptType::P2tr
    } else if script.as_bytes() == [0x51, 0x02, 0x4e, 0x73] {
        ScriptType::P2a
    } else if script.is_witness_program() {
        ScriptType::WitnessUnknown
    } else if script.is_op_return() {
        ScriptType::OpReturn
    } else if script.is_p2pk() {
        ScriptType::P2pk
    } else if script.is_multisig() {
        ScriptType::Multisig
    } else {
        ScriptType::Nonstandard
    }
}

/// The data pushed after `OP_RETURN`, concatenated.
pub fn op_return_data(script: &Script) -> Vec<u8> {
    script
        .instructions()
        .skip(1)
        .filter_map(|i| match i {
            Ok(Instruction::PushBytes(bytes)) => Some(bytes.as_bytes().to_vec()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// BIP141: the coinbase commits to all witnesses in an `OP_RETURN aa21a9ed…` output.
pub fn is_witness_commitment(script: &Script) -> bool {
    script
        .as_bytes()
        .starts_with(&[0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed])
}

/// How an input spends its previous output.
///
/// With the previous output's script the answer is certain. Without it, the input's
/// scriptSig and witness usually reveal the template, so we infer it and say so.
pub fn spend_type(input: &TxIn, prevout: Option<&TxOut>) -> (String, SpendTypeSource) {
    match prevout {
        Some(prevout) => {
            let spent = classify(&prevout.script_pubkey);
            let label = match spent {
                ScriptType::P2sh => nested_segwit(input).unwrap_or("p2sh"),
                ScriptType::P2tr => taproot_path(input),
                other => other.as_str(),
            };
            (label.to_owned(), SpendTypeSource::Prevout)
        }
        None => (infer(input).to_owned(), SpendTypeSource::Inferred),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendTypeSource {
    Prevout,
    Inferred,
}

fn infer(input: &TxIn) -> &'static str {
    let witness: Vec<&[u8]> = input.witness.iter().collect();
    let pushes = push_items(&input.script_sig);

    if input.previous_output.is_null() {
        return "coinbase";
    }
    if let Some(nested) = nested_segwit(input) {
        return nested;
    }
    if !input.script_sig.is_empty() && witness.is_empty() {
        return match pushes.as_deref() {
            Some([_signature, pubkey]) if matches!(pubkey.len(), 33 | 65) => "p2pkh",
            Some([_signature]) => "p2pk",
            Some(_) => "p2sh",
            None => "unknown",
        };
    }
    if input.script_sig.is_empty() {
        return match witness.as_slice() {
            [_signature, pubkey] if pubkey.len() == 33 && matches!(pubkey[0], 2 | 3) => "p2wpkh",
            [] => "unknown",
            _ if taproot_path(input) != "p2tr" => taproot_path(input),
            _ => "p2wsh",
        };
    }
    "unknown"
}

/// P2SH-wrapped SegWit: the scriptSig is a single push of a v0 witness program.
fn nested_segwit(input: &TxIn) -> Option<&'static str> {
    match push_items(&input.script_sig)?.as_slice() {
        [redeem] if redeem.len() == 22 && redeem.starts_with(&[0x00, 0x14]) => Some("p2sh-p2wpkh"),
        [redeem] if redeem.len() == 34 && redeem.starts_with(&[0x00, 0x20]) => Some("p2sh-p2wsh"),
        _ => None,
    }
}

/// Taproot spends are key path (one 64/65-byte signature) or script path (the
/// last item is a control block: 0xc0/0xc1 leaf version + 32-byte chunks).
fn taproot_path(input: &TxIn) -> &'static str {
    let mut items: Vec<&[u8]> = input.witness.iter().collect();
    // BIP341: a final item starting with 0x50 is the annex, not part of the spend.
    if items.len() >= 2 && items.last().is_some_and(|i| i.first() == Some(&0x50)) {
        items.pop();
    }
    match items.as_slice() {
        [signature] if matches!(signature.len(), 64 | 65) => "p2tr_key_path",
        [.., control]
            if control.len() >= 33
                && (control.len() - 33) % 32 == 0
                && control[0] & 0xfe == 0xc0 =>
        {
            "p2tr_script_path"
        }
        _ => "p2tr",
    }
}

/// The data items of a push-only script, or `None` if it contains other opcodes.
fn push_items(script: &Script) -> Option<Vec<Vec<u8>>> {
    script
        .instructions()
        .map(|i| match i {
            Ok(Instruction::PushBytes(bytes)) => Some(bytes.as_bytes().to_vec()),
            _ => None,
        })
        .collect()
}
