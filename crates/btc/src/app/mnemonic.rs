use btc_core::mnemonic::{self, MnemonicInfo, RootKey};
use zeroize::Zeroizing;

use super::input::{ensure_single_stdin, secret_arg};
use super::{AppError, Context};

pub fn new(
    ctx: &Context,
    words: usize,
    passphrase: Option<&str>,
) -> Result<MnemonicInfo, AppError> {
    let passphrase = passphrase_arg(passphrase)?;
    Ok(mnemonic::generate(words, &passphrase, ctx.network)?)
}

pub fn to_xprv(ctx: &Context, phrase: &str, passphrase: Option<&str>) -> Result<RootKey, AppError> {
    ensure_single_stdin(&[Some(phrase), passphrase])?;
    let phrase = secret_arg(phrase)?;
    let passphrase = passphrase_arg(passphrase)?;
    Ok(mnemonic::root_key(&phrase, &passphrase, ctx.network)?)
}

/// No passphrase is the same as an empty one (BIP39).
fn passphrase_arg(passphrase: Option<&str>) -> Result<Zeroizing<String>, AppError> {
    match passphrase {
        Some(value) => secret_arg(value),
        None => Ok(Zeroizing::new(String::new())),
    }
}
