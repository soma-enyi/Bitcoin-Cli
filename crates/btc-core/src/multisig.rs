//! M-of-N multisig redeem scripts and the addresses that commit to them.

use std::str::FromStr;

use bitcoin::opcodes::all::{OP_CHECKMULTISIG, OP_PUSHNUM_1, OP_PUSHNUM_16};
use bitcoin::script::{Builder, Instruction, Script, ScriptBuf};
use bitcoin::{Address, PublicKey};
use serde::Serialize;

use crate::error::CoreError;
use crate::network::Network;

pub const MAX_KEYS: usize = 15;

#[derive(Debug, Serialize)]
pub struct MultisigInfo {
    pub threshold: usize,
    pub total_keys: usize,
    pub pubkeys: Vec<String>,
    pub script_hex: String,
    pub p2sh: String,
    pub p2wsh: String,
    pub p2sh_p2wsh: String,
}

/// Builds `m <pubkeys> n OP_CHECKMULTISIG`. Keys are sorted (BIP67) unless
/// `keep_order`, so every signer derives the same address from the same key set.
pub fn create(
    threshold: usize,
    pubkeys: &[String],
    network: Network,
    keep_order: bool,
) -> Result<MultisigInfo, CoreError> {
    let total = pubkeys.len();
    if threshold == 0 || total == 0 || threshold > total {
        return Err(CoreError::InvalidInput(format!(
            "threshold must be between 1 and the number of keys ({total}), got {threshold}"
        )));
    }
    if total > MAX_KEYS {
        return Err(CoreError::InvalidInput(format!(
            "at most {MAX_KEYS} keys fit in a standard multisig, got {total}"
        )));
    }

    let mut keys = Vec::with_capacity(total);
    for hex in pubkeys {
        let key = PublicKey::from_str(hex.trim())
            .map_err(|e| CoreError::InvalidInput(format!("bad public key {hex}: {e}")))?;
        if !key.compressed {
            return Err(CoreError::InvalidInput(format!(
                "public key {hex} is uncompressed; segwit multisig needs compressed (02/03) keys"
            )));
        }
        if keys.contains(&key) {
            return Err(CoreError::InvalidInput(format!(
                "duplicate public key {hex}"
            )));
        }
        keys.push(key);
    }
    if !keep_order {
        keys.sort_by_key(|k| k.inner.serialize());
    }

    let mut builder = Builder::new().push_int(threshold as i64);
    for key in &keys {
        builder = builder.push_key(key);
    }
    let script = builder
        .push_int(total as i64)
        .push_opcode(OP_CHECKMULTISIG)
        .into_script();

    let net: bitcoin::Network = network.into();
    Ok(MultisigInfo {
        threshold,
        total_keys: total,
        pubkeys: keys.iter().map(|k| k.to_string()).collect(),
        script_hex: script.to_hex_string(),
        p2sh: Address::p2sh(&script, net)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?
            .to_string(),
        p2wsh: Address::p2wsh(&script, net).to_string(),
        p2sh_p2wsh: Address::p2shwsh(&script, net).to_string(),
    })
}

/// Reads a bare multisig script back into `(threshold, pubkeys)`.
pub fn parse(script_hex: &str) -> Result<(usize, Vec<String>), CoreError> {
    let bytes = hex_to_bytes(script_hex)?;
    let script = ScriptBuf::from_bytes(bytes);
    parse_script(&script)
}

fn parse_script(script: &Script) -> Result<(usize, Vec<String>), CoreError> {
    let not_multisig = || CoreError::InvalidInput("not an m-of-n OP_CHECKMULTISIG script".into());
    let ops: Vec<_> = script
        .instructions()
        .collect::<Result<_, _>>()
        .map_err(|e| CoreError::InvalidInput(format!("malformed script: {e}")))?;
    if ops.len() < 4 {
        return Err(not_multisig());
    }
    let small = |i: &Instruction| match i {
        Instruction::Op(op)
            if (OP_PUSHNUM_1.to_u8()..=OP_PUSHNUM_16.to_u8()).contains(&op.to_u8()) =>
        {
            Some((op.to_u8() - OP_PUSHNUM_1.to_u8() + 1) as usize)
        }
        _ => None,
    };
    let m = small(&ops[0]).ok_or_else(not_multisig)?;
    let n = small(&ops[ops.len() - 2]).ok_or_else(not_multisig)?;
    if !matches!(&ops[ops.len() - 1], Instruction::Op(op) if *op == OP_CHECKMULTISIG)
        || ops.len() != n + 3
        || m > n
    {
        return Err(not_multisig());
    }
    let mut keys = Vec::with_capacity(n);
    for op in &ops[1..=n] {
        match op {
            Instruction::PushBytes(b) => keys.push(
                PublicKey::from_slice(b.as_bytes())
                    .map_err(|e| CoreError::InvalidInput(format!("bad key in script: {e}")))?
                    .to_string(),
            ),
            _ => return Err(not_multisig()),
        }
    }
    Ok((m, keys))
}

fn hex_to_bytes(s: &str) -> Result<Vec<u8>, CoreError> {
    let s = s.trim();
    if s.len() % 2 != 0 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidInput(
            "script must be an even-length hex string".into(),
        ));
    }
    Ok((0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The three keys from the 2-of-3 example in BIP11/BIP67-style tests.
    const K1: &str = "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
    const K2: &str = "03a34b99f22c790c4e36b2b3c2c35a36db06226e41c692fc82b8b56ac1c540c5bd";
    const K3: &str = "025cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc";

    fn keys() -> Vec<String> {
        vec![K1.into(), K2.into(), K3.into()]
    }

    #[test]
    fn builds_a_two_of_three_script_that_round_trips() {
        let info = create(2, &keys(), Network::Regtest, false).unwrap();
        assert!(info.script_hex.starts_with("52"));
        assert!(info.script_hex.ends_with("53ae"));
        let (m, parsed) = parse(&info.script_hex).unwrap();
        assert_eq!(m, 2);
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn key_order_does_not_change_the_address_when_sorted() {
        let a = create(2, &keys(), Network::Regtest, false).unwrap();
        let mut rev = keys();
        rev.reverse();
        let b = create(2, &rev, Network::Regtest, false).unwrap();
        assert_eq!(a.p2wsh, b.p2wsh);
        assert_eq!(a.script_hex, b.script_hex);
    }

    #[test]
    fn addresses_match_the_network() {
        let info = create(2, &keys(), Network::Regtest, false).unwrap();
        assert!(info.p2wsh.starts_with("bcrt1"));
        assert!(info.p2sh.starts_with('2'));
    }

    #[test]
    fn rejects_bad_parameters() {
        assert!(create(0, &keys(), Network::Regtest, false).is_err());
        assert!(create(4, &keys(), Network::Regtest, false).is_err());
        assert!(create(1, &[], Network::Regtest, false).is_err());
        assert!(create(1, &["zz".into()], Network::Regtest, false).is_err());
        let dup = vec![K1.to_string(), K1.to_string()];
        assert!(create(1, &dup, Network::Regtest, false).is_err());
        let many: Vec<String> = (0..16).map(|_| K1.to_string()).collect();
        assert!(create(1, &many, Network::Regtest, false).is_err());
    }

    #[test]
    fn parse_rejects_non_multisig() {
        assert!(parse("76a914").is_err());
        assert!(parse("abc").is_err());
    }
}
