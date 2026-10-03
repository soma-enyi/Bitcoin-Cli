use bitcoin::Address;
use bitcoin::OutPoint;
use bitcoin::Sequence;
use bitcoin::Transaction;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitcoin::Txid;
use bitcoin::amount::Amount;
use bitcoin::blockdata::script::ScriptBuf;
use bitcoin::psbt::Psbt;

use crate::Network;
use crate::error::CoreError;

#[derive(Clone, Debug)]
pub struct TxInput {
    pub txid: Txid,
    pub vout: u32,
    pub amount_sats: Option<u64>,
    pub script_pubkey: Option<ScriptBuf>,
}

impl TxInput {
    /// `txid:vout[:sats[:address]]`. The address is the output being spent: with it and the
    /// amount the PSBT carries everything a signer needs (BIP143/BIP341 hash the amount and script).
    pub fn parse(s: &str, network: Network) -> Result<Self, CoreError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 || parts.len() > 4 {
            return Err(CoreError::InvalidInput(
                "input format: txid:vout[:amount_sats[:address]]".into(),
            ));
        }

        let txid = parts[0]
            .parse()
            .map_err(|_| CoreError::InvalidInput(format!("invalid txid: {}", parts[0])))?;

        let vout = parts[1]
            .parse()
            .map_err(|_| CoreError::InvalidInput(format!("invalid vout: {}", parts[1])))?;

        let amount_sats = match parts.get(2) {
            Some(amount) => Some(
                amount
                    .parse()
                    .map_err(|_| CoreError::InvalidInput(format!("invalid amount: {amount}")))?,
            ),
            None => None,
        };

        let script_pubkey = match parts.get(3) {
            Some(address) => {
                if amount_sats.is_none() {
                    return Err(CoreError::InvalidInput(
                        "an input address needs an amount: txid:vout:sats:address".into(),
                    ));
                }
                let script = parse_address(address, network)?.script_pubkey();
                if !script.is_witness_program() {
                    return Err(CoreError::InvalidInput(format!(
                        "input {address} is a legacy (non-segwit) address: signing it needs the \
                         full previous transaction, which this tool does not support. Use a \
                         native segwit (bc1q/bcrt1q/tb1q) or taproot (…1p) address"
                    )));
                }
                Some(script)
            }
            None => None,
        };

        Ok(TxInput {
            txid,
            vout,
            amount_sats,
            script_pubkey,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TxOutput {
    pub address: Address,
    pub amount_sats: u64,
}

impl TxOutput {
    pub fn parse(s: &str, network: Network) -> Result<Self, CoreError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(CoreError::InvalidInput(
                "output format: address:amount_sats".into(),
            ));
        }

        let address = parse_address(parts[0], network)?;

        let amount_sats = parts[1]
            .parse()
            .map_err(|_| CoreError::InvalidInput(format!("invalid amount: {}", parts[1])))?;

        Ok(TxOutput {
            address,
            amount_sats,
        })
    }
}

/// Parses an address and insists it belongs to `network`, so a regtest address can
/// never end up in a mainnet transaction (or the other way round).
pub fn parse_address(s: &str, network: Network) -> Result<Address, CoreError> {
    let unchecked = s
        .trim()
        .parse::<Address<bitcoin::address::NetworkUnchecked>>()
        .map_err(|e| CoreError::InvalidAddress(format!("invalid address: {e}")))?;
    unchecked
        .require_network(bitcoin::Network::from(network))
        .map_err(|_| CoreError::InvalidAddress(format!("{s} is not a valid {network} address")))
}

pub fn create_psbt(
    inputs: Vec<TxInput>,
    mut outputs: Vec<TxOutput>,
    change_address: Option<Address>,
    fee_rate: f64,
) -> Result<Psbt, CoreError> {
    if inputs.is_empty() {
        return Err(CoreError::InvalidInput(
            "at least one input required".into(),
        ));
    }
    if outputs.is_empty() {
        return Err(CoreError::InvalidInput(
            "at least one output required".into(),
        ));
    }

    let total_input_sats: u64 = inputs.iter().filter_map(|i| i.amount_sats).sum();

    // 21 million BTC is the most that can exist; anything above is a typo, and it keeps the
    // sums below from overflowing.
    const MAX_SATS: u64 = 21_000_000 * 100_000_000;
    if outputs.iter().any(|o| o.amount_sats > MAX_SATS) || total_input_sats > MAX_SATS {
        return Err(CoreError::InvalidInput(
            "an amount is larger than the 21 million BTC that can exist".into(),
        ));
    }
    let total_output_sats: u64 = outputs.iter().map(|o| o.amount_sats).sum();

    if total_input_sats == 0 {
        return Err(CoreError::InvalidInput(
            "cannot calculate fee: all inputs lack amounts. Use --input txid:vout:amount".into(),
        ));
    }

    let dust_limit = 546;
    for output in &outputs {
        if output.amount_sats < dust_limit {
            return Err(CoreError::InvalidInput(format!(
                "output {} sats is dust (< {})",
                output.amount_sats, dust_limit
            )));
        }
    }

    let mut output_script_lens: Vec<usize> = outputs
        .iter()
        .map(|o| o.address.script_pubkey().len())
        .collect();
    if let Some(change) = &change_address {
        output_script_lens.push(change.script_pubkey().len());
    }
    let estimated_vsize = estimate_vsize(&inputs, &output_script_lens);
    let estimated_fee = ((estimated_vsize as f64) * fee_rate).ceil() as u64;

    if total_input_sats < total_output_sats.saturating_add(estimated_fee) {
        return Err(CoreError::InvalidInput(format!(
            "insufficient funds: {} sats input < {} sats output + {} sats fee",
            total_input_sats, total_output_sats, estimated_fee
        )));
    }

    if estimated_fee > total_input_sats / 10 {
        return Err(CoreError::InvalidInput(format!(
            "absurd fee: {} sats > 10% of inputs ({})",
            estimated_fee, total_input_sats
        )));
    }

    if let Some(change_addr) = change_address {
        let change_amount = total_input_sats - total_output_sats - estimated_fee;
        if change_amount >= dust_limit {
            outputs.push(TxOutput {
                address: change_addr,
                amount_sats: change_amount,
            });
        }
    }

    let tx_inputs: Vec<TxIn> = inputs
        .iter()
        .map(|i| TxIn {
            previous_output: OutPoint {
                txid: i.txid,
                vout: i.vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        })
        .collect();

    let tx_outputs: Vec<TxOut> = outputs
        .iter()
        .map(|o| TxOut {
            value: Amount::from_sat(o.amount_sats),
            script_pubkey: o.address.script_pubkey(),
        })
        .collect();

    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
        input: tx_inputs,
        output: tx_outputs,
    };

    let mut psbt = Psbt::from_unsigned_tx(tx)
        .map_err(|e| CoreError::InvalidInput(format!("PSBT creation failed: {}", e)))?;

    for (slot, input) in psbt.inputs.iter_mut().zip(&inputs) {
        if let (Some(amount), Some(script)) = (input.amount_sats, &input.script_pubkey) {
            slot.witness_utxo = Some(TxOut {
                value: Amount::from_sat(amount),
                script_pubkey: script.clone(),
            });
        }
    }
    Ok(psbt)
}

/// The size the signed transaction will have, so the fee pays the rate asked for.
///
/// Per input: 41 bytes of outpoint, empty script and sequence, plus the witness that
/// `tx sign` produces: 108 weight units for P2WPKH (signature up to 72 bytes, 33-byte key) or
/// 66 for a taproot key spend. Per output: 8 value bytes, a length byte and the script.
/// A transaction with a witness also carries a 2-byte marker and flag.
fn estimate_vsize(inputs: &[TxInput], output_script_lens: &[usize]) -> usize {
    let overhead_wu = (4 + 1 + 1 + 4) * 4 + 2;
    let inputs_wu: usize = inputs
        .iter()
        .map(|input| {
            let witness_wu = match &input.script_pubkey {
                Some(script) if script.is_p2tr() => 66,
                _ => 108,
            };
            41 * 4 + witness_wu
        })
        .sum();
    let outputs_wu: usize = output_script_lens.iter().map(|len| (8 + 1 + len) * 4).sum();
    (overhead_wu + inputs_wu + outputs_wu).div_ceil(4)
}

#[cfg(test)]
mod tests {
    #[test]
    fn addresses_must_belong_to_the_selected_network() {
        let main = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
        assert!(parse_address(main, Network::Mainnet).is_ok());
        let err = parse_address(main, Network::Regtest)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("not a valid regtest address"), "{err}");
        assert!(TxOutput::parse(&format!("{main}:1000"), Network::Signet).is_err());
        assert!(TxOutput::parse(&format!("{main}:1000"), Network::Mainnet).is_ok());
    }

    use super::*;
    use bitcoin::hashes::Hash;

    #[test]
    fn the_size_estimate_matches_real_transactions() {
        let p2wpkh = ScriptBuf::new_p2wpkh(&bitcoin::WPubkeyHash::from_byte_array([1; 20]));
        let input = |script: &ScriptBuf| TxInput {
            txid: Txid::from_byte_array([1; 32]),
            vout: 0,
            amount_sats: Some(1),
            script_pubkey: Some(script.clone()),
        };
        // The well-known size of a 1-input, 2-output P2WPKH payment is 141 vB.
        assert_eq!(estimate_vsize(&[input(&p2wpkh)], &[22, 22]), 141);
        // 1 in, 1 out: 110 vB.
        assert_eq!(estimate_vsize(&[input(&p2wpkh)], &[22]), 110);
        let p2tr =
            ScriptBuf::new_p2tr_tweaked(bitcoin::key::TweakedPublicKey::dangerous_assume_tweaked(
                bitcoin::XOnlyPublicKey::from_slice(&[2; 32]).unwrap_or_else(|_| {
                    bitcoin::secp256k1::Secp256k1::new()
                        .generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng())
                        .1
                        .x_only_public_key()
                        .0
                }),
            ));
        // A taproot key spend is cheaper than P2WPKH, and a taproot output costs more.
        assert!(estimate_vsize(&[input(&p2tr)], &[22]) < 110);
        assert!(estimate_vsize(&[input(&p2wpkh)], &[34]) > 110);
    }

    #[test]
    fn huge_amounts_are_refused_instead_of_overflowing() {
        let address = parse_address(
            "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080",
            Network::Regtest,
        )
        .unwrap();
        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        let input = TxInput::parse(&format!("{zero}:0:1000000"), Network::Regtest).unwrap();
        let output = |sats| TxOutput {
            address: address.clone(),
            amount_sats: sats,
        };
        for sats in [u64::MAX, u64::MAX - 100, 21_000_001 * 100_000_000] {
            let err = create_psbt(
                vec![input.clone()],
                vec![output(sats)],
                Some(address.clone()),
                1.0,
            );
            assert!(err.is_err(), "{sats} was accepted");
        }
    }

    #[test]
    fn test_input_parsing() {
        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        let input = TxInput::parse(&format!("{zero}:0:1000"), Network::Regtest).unwrap();
        assert_eq!(input.vout, 0);
        assert_eq!(input.amount_sats, Some(1000));
        assert!(input.script_pubkey.is_none());
    }

    const G: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn address_of(kind: crate::address::AddressType) -> String {
        crate::address::from_pubkey_hex(G, Some(kind), Network::Regtest)
            .unwrap()
            .addresses[0]
            .address
            .clone()
    }

    #[test]
    fn input_address_must_be_segwit_on_the_right_network() {
        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        let segwit = address_of(crate::address::AddressType::P2wpkh);
        let input = TxInput::parse(&format!("{zero}:1:5000:{segwit}"), Network::Regtest).unwrap();
        assert!(input.script_pubkey.unwrap().is_p2wpkh());
        assert!(TxInput::parse(&format!("{zero}:1:5000:{segwit}"), Network::Mainnet).is_err());
        let legacy = address_of(crate::address::AddressType::P2pkh);
        let err = TxInput::parse(&format!("{zero}:1:5000:{legacy}"), Network::Regtest)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("legacy"), "{err}");
        assert!(TxInput::parse(&format!("{zero}:1"), Network::Regtest).is_ok());
        assert!(TxInput::parse(&format!("{zero}:1::{segwit}"), Network::Regtest).is_err());
    }

    #[test]
    fn create_psbt_records_the_prevout_for_signers() {
        let zero = "0000000000000000000000000000000000000000000000000000000000000000";
        let segwit = address_of(crate::address::AddressType::P2wpkh);
        let input = TxInput::parse(&format!("{zero}:0:100000:{segwit}"), Network::Regtest).unwrap();
        let out = TxOutput::parse(&format!("{segwit}:50000"), Network::Regtest).unwrap();
        let psbt = create_psbt(vec![input], vec![out], None, 2.0).unwrap();
        let utxo = psbt.inputs[0].witness_utxo.as_ref().unwrap();
        assert_eq!(utxo.value.to_sat(), 100_000);
        assert!(utxo.script_pubkey.is_p2wpkh());
    }
}
