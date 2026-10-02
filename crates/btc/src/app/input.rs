//! Reading secrets without putting them on the command line, where they would be
//! saved in shell history and visible to other users through `ps`.

use std::io::{self, Read};

use zeroize::Zeroizing;

use super::AppError;

/// The argument value that means "read this from stdin instead".
pub const STDIN: &str = "-";

/// Returns the argument itself, or reads it from stdin when it is `-`
/// (e.g. `cat words.txt | btc mnemonic to-xprv -`).
pub fn secret_arg(value: &str) -> Result<Zeroizing<String>, AppError> {
    if value != STDIN {
        return Ok(Zeroizing::new(value.to_owned()));
    }

    let mut buf = Zeroizing::new(String::new());
    io::stdin().read_to_string(&mut buf)?;
    let len = buf.trim_end_matches(['\r', '\n']).len();
    buf.truncate(len);
    Ok(buf)
}

/// Like [`secret_arg`] for values that aren't secret, e.g. transaction hex.
pub fn text_arg(value: &str) -> Result<String, AppError> {
    Ok(secret_arg(value)?.to_string())
}

/// Stdin can only be read once, so at most one argument may be `-`.
pub fn ensure_single_stdin(values: &[Option<&str>]) -> Result<(), AppError> {
    if values.iter().flatten().filter(|v| **v == STDIN).count() > 1 {
        return Err(AppError::Input(
            "only one argument can be read from stdin (`-`)".into(),
        ));
    }
    Ok(())
}
