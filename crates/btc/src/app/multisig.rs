use serde::Serialize;

use btc_core::multisig::{self, MultisigInfo};

use super::input::text_arg;
use super::{AppError, Context};
use crate::output::Render;

impl Render for MultisigInfo {
    fn render_human(&self) -> String {
        let mut out = format!("{}-of-{} multisig\n\n", self.threshold, self.total_keys);
        for (i, key) in self.pubkeys.iter().enumerate() {
            out.push_str(&format!("key {}        {key}\n", i + 1));
        }
        out.push_str(&format!(
            "\nscript       {}\np2sh         {}\np2wsh        {}\np2sh-p2wsh   {}",
            self.script_hex, self.p2sh, self.p2wsh, self.p2sh_p2wsh
        ));
        out
    }
}

#[derive(Serialize)]
pub struct MultisigAnalysis {
    pub threshold: usize,
    pub total_keys: usize,
    pub pubkeys: Vec<String>,
}

impl Render for MultisigAnalysis {
    fn render_human(&self) -> String {
        let mut out = format!("{}-of-{} multisig script\n", self.threshold, self.total_keys);
        for (i, key) in self.pubkeys.iter().enumerate() {
            out.push_str(&format!("key {}  {key}\n", i + 1));
        }
        out.trim_end().to_owned()
    }
}

pub fn create(
    ctx: &Context,
    threshold: usize,
    pubkeys: &[String],
    keep_order: bool,
) -> Result<MultisigInfo, AppError> {
    Ok(multisig::create(threshold, pubkeys, ctx.network, keep_order)?)
}

pub fn analyze(_ctx: &Context, script: &str) -> Result<MultisigAnalysis, AppError> {
    let (threshold, pubkeys) = multisig::parse(&text_arg(script)?)?;
    Ok(MultisigAnalysis {
        threshold,
        total_keys: pubkeys.len(),
        pubkeys,
    })
}
