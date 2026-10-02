use std::str::FromStr;

use bitcoin::psbt::Psbt;
use serde::Serialize;

use super::input::text_arg;
use super::{AppError, Context};
use crate::output::Render;

#[derive(Serialize)]
pub struct PsbtInput {
    pub outpoint: String,
    pub partial_signatures: usize,
    pub finalized: bool,
    pub amount_sat: Option<u64>,
}

#[derive(Serialize)]
pub struct PsbtOutputInfo {
    pub amount_sat: u64,
    pub script_hex: String,
}

#[derive(Serialize)]
pub struct PsbtInfo {
    pub txid: String,
    pub version: i32,
    pub inputs: Vec<PsbtInput>,
    pub outputs: Vec<PsbtOutputInfo>,
    pub fee_sat: Option<u64>,
    pub all_finalized: bool,
}

impl Render for PsbtInfo {
    fn render_human(&self) -> String {
        let mut out = format!("txid     {}\nversion  {}\n\n", self.txid, self.version);
        for (i, input) in self.inputs.iter().enumerate() {
            let amount = input
                .amount_sat
                .map_or("unknown".to_owned(), |a| format!("{a} sat"));
            out.push_str(&format!(
                "input {i}   {}  {amount}  sigs: {}  {}\n",
                input.outpoint,
                input.partial_signatures,
                if input.finalized {
                    "finalized"
                } else {
                    "not finalized"
                }
            ));
        }
        for (i, output) in self.outputs.iter().enumerate() {
            out.push_str(&format!(
                "output {i}  {} sat  {}\n",
                output.amount_sat, output.script_hex
            ));
        }
        match self.fee_sat {
            Some(fee) => out.push_str(&format!("\nfee      {fee} sat")),
            None => out.push_str("\nfee      unknown (inputs are missing amounts)"),
        }
        out
    }
}

#[derive(Serialize)]
pub struct PsbtCombined {
    pub psbt: String,
}

impl Render for PsbtCombined {
    fn render_human(&self) -> String {
        self.psbt.clone()
    }
}

fn parse(text: &str) -> Result<Psbt, AppError> {
    Psbt::from_str(text.trim()).map_err(|e| {
        AppError::Input(format!(
            "invalid PSBT: {e}. Expected base64 starting with `cHNidP8`"
        ))
    })
}

pub fn analyze(_ctx: &Context, psbt: &str) -> Result<PsbtInfo, AppError> {
    let psbt = parse(&text_arg(psbt)?)?;
    let tx = &psbt.unsigned_tx;

    let mut inputs = Vec::new();
    for (txin, data) in tx.input.iter().zip(&psbt.inputs) {
        let amount_sat = psbt_input_amount(txin, data);
        inputs.push(PsbtInput {
            outpoint: txin.previous_output.to_string(),
            partial_signatures: data.partial_sigs.len(),
            finalized: data.final_script_sig.is_some() || data.final_script_witness.is_some(),
            amount_sat,
        });
    }
    let outputs: Vec<_> = tx
        .output
        .iter()
        .map(|o| PsbtOutputInfo {
            amount_sat: o.value.to_sat(),
            script_hex: o.script_pubkey.to_hex_string(),
        })
        .collect();

    let fee_sat = inputs
        .iter()
        .map(|i| i.amount_sat)
        .sum::<Option<u64>>()
        .and_then(|total_in| total_in.checked_sub(outputs.iter().map(|o| o.amount_sat).sum()));

    Ok(PsbtInfo {
        txid: tx.compute_txid().to_string(),
        version: tx.version.0,
        all_finalized: inputs.iter().all(|i| i.finalized),
        inputs,
        outputs,
        fee_sat,
    })
}

fn psbt_input_amount(txin: &bitcoin::TxIn, data: &bitcoin::psbt::Input) -> Option<u64> {
    if let Some(utxo) = &data.witness_utxo {
        return Some(utxo.value.to_sat());
    }
    let prev = data.non_witness_utxo.as_ref()?;
    prev.output
        .get(txin.previous_output.vout as usize)
        .map(|o| o.value.to_sat())
}

pub fn combine(_ctx: &Context, psbts: &[String]) -> Result<PsbtCombined, AppError> {
    let mut iter = psbts.iter();
    let first = iter
        .next()
        .ok_or_else(|| AppError::Input("provide at least two PSBTs to combine".into()))?;
    let mut base = parse(&text_arg(first)?)?;
    for other in iter {
        let other = parse(&text_arg(other)?)?;
        base.combine(other).map_err(|e| {
            AppError::Input(format!(
                "cannot combine: {e} (the PSBTs must be for the same transaction)"
            ))
        })?;
    }
    Ok(PsbtCombined {
        psbt: base.to_string(),
    })
}
