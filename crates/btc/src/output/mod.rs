//! Presentation. Services return data; this module turns it into human-readable
//! text or JSON. Adding a command's output means implementing `Render` in `human.rs`.

mod human;

use serde::Serialize;
use serde_json::json;

use crate::app::AppError;

#[derive(Clone, Copy)]
pub enum OutputMode {
    Human,
    Json,
}

/// A command result that can be shown to a person. JSON output comes for free
/// from `Serialize`.
pub trait Render: Serialize {
    fn render_human(&self) -> String;

    /// True when the output includes private keys or mnemonics, so the user is warned.
    fn contains_secrets(&self) -> bool {
        false
    }

    /// True when the output belongs to mainnet, where secrets control real funds.
    fn is_mainnet(&self) -> bool {
        false
    }
}

pub fn emit<T: Render>(value: &T, mode: OutputMode) -> Result<(), AppError> {
    match mode {
        OutputMode::Human => {
            println!("{}", value.render_human());
            if value.contains_secrets() {
                // stderr, so the warning never ends up in a redirected file or pipe.
                if value.is_mainnet() {
                    eprintln!(
                        "\nWARNING: MAINNET secret key material. Anyone who sees this can spend \
                         real bitcoin. Do not paste it into chat, a screenshot or a shared \
                         terminal; clear your scrollback and shell history after use."
                    );
                } else {
                    eprintln!(
                        "\nwarning: this output contains secret key material. Anyone who sees it \
                         can spend the funds. Never reuse these keys on mainnet."
                    );
                }
            }
        }
        OutputMode::Json => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

/// Human errors go to stderr. JSON errors go to stdout so scripts can always parse
/// stdout, whether the command succeeded or not.
pub fn emit_error(err: &AppError, mode: OutputMode) {
    match mode {
        OutputMode::Human => eprintln!("error: {err}"),
        OutputMode::Json => {
            let body = json!({ "error": { "kind": err.kind(), "message": err.to_string() } });
            println!("{body:#}");
        }
    }
}
