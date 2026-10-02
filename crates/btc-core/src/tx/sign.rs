//! Signing PSBT inputs that pay to a single key: P2WPKH (BIP143) and taproot key-path (BIP341).

use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak};
use bitcoin::psbt::Psbt;
use bitcoin::secp256k1::{Message, Secp256k1, SecretKey};
use bitcoin::sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType};
use bitcoin::{CompressedPublicKey, ScriptBuf, Transaction, TxOut, Witness, ecdsa, taproot};

use crate::error::CoreError;

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidInput(message.into())
}

/// The output each input spends, which the signature hash commits to.
fn prevouts(psbt: &Psbt) -> Result<Vec<TxOut>, CoreError> {
    psbt.inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            input.witness_utxo.clone().ok_or_else(|| {
                invalid(format!(
                    "input {i} has no prevout amount/script, so it cannot be signed. Create the \
                     PSBT with --input txid:vout:sats:address"
                ))
            })
        })
        .collect()
}

/// Signs and finalizes every input that `secret` controls. Returns how many it signed.
pub fn sign_psbt(psbt: &mut Psbt, secret: &SecretKey) -> Result<usize, CoreError> {
    let secp = Secp256k1::new();
    let prevouts = prevouts(psbt)?;

    let keypair = Keypair::from_secret_key(&secp, secret);
    let public_key = CompressedPublicKey(secret.public_key(&secp));
    let p2wpkh = ScriptBuf::new_p2wpkh(&public_key.wpubkey_hash());
    let tweaked = keypair.tap_tweak(&secp, None);
    let (output_key, _) = keypair.x_only_public_key().0.tap_tweak(&secp, None);
    let p2tr = ScriptBuf::new_p2tr_tweaked(output_key);

    let tx = psbt.unsigned_tx.clone();
    let mut cache = SighashCache::new(&tx);
    let mut signed = 0;

    for (index, utxo) in prevouts.iter().enumerate() {
        if psbt.inputs[index].final_script_witness.is_some() {
            continue;
        }
        let witness = if utxo.script_pubkey == p2wpkh {
            let hash = cache
                .p2wpkh_signature_hash(
                    index,
                    &utxo.script_pubkey,
                    utxo.value,
                    EcdsaSighashType::All,
                )
                .map_err(|e| invalid(format!("input {index}: {e}")))?;
            let signature = ecdsa::Signature {
                signature: secp.sign_ecdsa(&Message::from_digest(hash.to_byte_array()), secret),
                sighash_type: EcdsaSighashType::All,
            };
            Witness::p2wpkh(&signature, &public_key.0)
        } else if utxo.script_pubkey == p2tr {
            let hash = cache
                .taproot_key_spend_signature_hash(
                    index,
                    &Prevouts::All(&prevouts),
                    TapSighashType::Default,
                )
                .map_err(|e| invalid(format!("input {index}: {e}")))?;
            let signature = taproot::Signature {
                signature: secp.sign_schnorr(
                    &Message::from_digest(hash.to_byte_array()),
                    &tweaked.to_keypair(),
                ),
                sighash_type: TapSighashType::Default,
            };
            Witness::p2tr_key_spend(&signature)
        } else {
            continue;
        };
        psbt.inputs[index].final_script_witness = Some(witness);
        signed += 1;
    }

    if signed == 0 {
        return Err(invalid(
            "this key does not control any input of the PSBT (inputs must pay to this key's \
             P2WPKH or taproot address)",
        ));
    }
    Ok(signed)
}

/// The final transaction, once every input is signed.
pub fn extract_signed(psbt: Psbt) -> Result<Transaction, CoreError> {
    let unsigned: Vec<usize> = psbt
        .inputs
        .iter()
        .enumerate()
        .filter(|(_, i)| i.final_script_witness.is_none() && i.final_script_sig.is_none())
        .map(|(n, _)| n)
        .collect();
    if !unsigned.is_empty() {
        return Err(invalid(format!(
            "input(s) {unsigned:?} are still unsigned: this key signs only inputs paying to its \
             own address"
        )));
    }
    psbt.extract_tx()
        .map_err(|e| invalid(format!("cannot build the final transaction: {e}")))
}

#[cfg(test)]
mod tests {
    use bitcoin::absolute::LockTime;
    use bitcoin::transaction::Version;
    use bitcoin::{Amount, OutPoint, Sequence, TxIn, Txid};

    use super::*;

    fn secret(byte: u8) -> SecretKey {
        SecretKey::from_slice(&[byte; 32]).unwrap()
    }

    fn psbt_spending(script: ScriptBuf, amount: u64) -> Psbt {
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_byte_array([7; 32]),
                    vout: 1,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(amount - 500),
                script_pubkey: script.clone(),
            }],
        };
        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        psbt.inputs[0].witness_utxo = Some(TxOut {
            value: Amount::from_sat(amount),
            script_pubkey: script,
        });
        psbt
    }

    fn p2wpkh_for(secret: &SecretKey) -> ScriptBuf {
        let secp = Secp256k1::new();
        ScriptBuf::new_p2wpkh(&CompressedPublicKey(secret.public_key(&secp)).wpubkey_hash())
    }

    #[test]
    fn p2wpkh_signature_verifies_against_the_bip143_hash() {
        let secp = Secp256k1::new();
        let key = secret(1);
        let script = p2wpkh_for(&key);
        let mut psbt = psbt_spending(script.clone(), 100_000);
        assert_eq!(sign_psbt(&mut psbt, &key).unwrap(), 1);
        let tx = extract_signed(psbt).unwrap();

        let witness: Vec<&[u8]> = tx.input[0].witness.iter().collect();
        assert_eq!(witness.len(), 2);
        let (sig_bytes, pubkey) = (witness[0], witness[1]);
        assert_eq!(*sig_bytes.last().unwrap(), 0x01, "SIGHASH_ALL");
        let sig = bitcoin::secp256k1::ecdsa::Signature::from_der(&sig_bytes[..sig_bytes.len() - 1])
            .unwrap();

        let hash = SighashCache::new(&tx)
            .p2wpkh_signature_hash(0, &script, Amount::from_sat(100_000), EcdsaSighashType::All)
            .unwrap();
        let msg = Message::from_digest(hash.to_byte_array());
        let pk = bitcoin::secp256k1::PublicKey::from_slice(pubkey).unwrap();
        assert!(secp.verify_ecdsa(&msg, &sig, &pk).is_ok());
    }

    #[test]
    fn taproot_key_path_signature_verifies_against_the_bip341_hash() {
        let secp = Secp256k1::new();
        let key = secret(2);
        let keypair = Keypair::from_secret_key(&secp, &key);
        let (output_key, _) = keypair.x_only_public_key().0.tap_tweak(&secp, None);
        let script = ScriptBuf::new_p2tr_tweaked(output_key);
        let prevout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: script.clone(),
        };
        let mut psbt = psbt_spending(script, 100_000);
        sign_psbt(&mut psbt, &key).unwrap();
        let tx = extract_signed(psbt).unwrap();

        let witness: Vec<&[u8]> = tx.input[0].witness.iter().collect();
        assert_eq!(witness.len(), 1);
        assert_eq!(
            witness[0].len(),
            64,
            "SIGHASH_DEFAULT signatures have no suffix byte"
        );
        let sig = bitcoin::secp256k1::schnorr::Signature::from_slice(witness[0]).unwrap();

        let hash = SighashCache::new(&tx)
            .taproot_key_spend_signature_hash(
                0,
                &Prevouts::All(&[prevout]),
                TapSighashType::Default,
            )
            .unwrap();
        let msg = Message::from_digest(hash.to_byte_array());
        assert!(
            secp.verify_schnorr(&sig, &msg, &output_key.to_x_only_public_key())
                .is_ok()
        );
    }

    #[test]
    fn a_key_that_does_not_own_the_input_signs_nothing() {
        let mut psbt = psbt_spending(p2wpkh_for(&secret(1)), 100_000);
        let err = sign_psbt(&mut psbt, &secret(9)).unwrap_err().to_string();
        assert!(err.contains("does not control any input"), "{err}");
        assert!(psbt.inputs[0].final_script_witness.is_none());
    }

    #[test]
    fn inputs_without_prevout_data_are_refused() {
        let mut psbt = psbt_spending(p2wpkh_for(&secret(1)), 100_000);
        psbt.inputs[0].witness_utxo = None;
        let err = sign_psbt(&mut psbt, &secret(1)).unwrap_err().to_string();
        assert!(err.contains("no prevout"), "{err}");
    }

    #[test]
    fn unsigned_psbts_cannot_be_extracted() {
        let psbt = psbt_spending(p2wpkh_for(&secret(1)), 100_000);
        assert!(extract_signed(psbt).is_err());
    }
}
