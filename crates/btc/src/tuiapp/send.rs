//! The Send tab's form and its stages: compose, review, enter the key, confirm, done.

use std::fmt;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::app::wallets::Wallet;

/// A secret typed into the TUI. Wiped when dropped, never printed, never saved.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Default for Secret {
    /// Room for a 24-word phrase up front: a `String` that grows leaves unwiped copies behind.
    fn default() -> Self {
        Secret(Zeroizing::new(String::with_capacity(512)))
    }
}

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn push(&mut self, c: char) {
        self.0.push(c);
    }
    pub fn pop(&mut self) {
        self.0.pop();
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn len(&self) -> usize {
        self.0.chars().count()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Where a payment is in its life. The data each later stage needs rides along.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SendStage {
    /// Filling in the form.
    Compose,
    /// The unsigned PSBT is built; the user checks the summary.
    Review { psbt: String },
    /// Waiting for the private key.
    Key { psbt: String },
    /// Signed; waiting for the go-ahead to broadcast.
    Confirm { hex: String, txid: String },
    /// Broadcast.
    Done,
}

pub const FIELD_TO: usize = 0;
pub const FIELD_AMOUNT: usize = 1;
pub const FIELD_FEE: usize = 2;
const FIELDS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct SendForm {
    pub to: String,
    pub amount: String,
    /// Blank asks the node for a rate.
    pub fee: String,
    pub focus: usize,
    pub stage: SendStage,
    /// The summary of the built payment, kept while the key is entered.
    pub summary: String,
    /// Mainnet only: the user must type `yes` before anything is broadcast.
    pub confirm: String,
    /// Bumped whenever the payment is edited, discarded or reset, so a job that was started
    /// before cannot deliver its result into the new state.
    pub epoch: u64,
    #[serde(skip)]
    pub key: Secret,
    /// The wallet the payment is made from, fixed when the user presses Enter.
    #[serde(skip)]
    pub wallet: Option<Wallet>,
}

impl SendForm {
    pub fn new() -> Self {
        SendForm {
            to: String::new(),
            amount: String::new(),
            fee: String::new(),
            focus: FIELD_TO,
            stage: SendStage::Compose,
            summary: String::new(),
            confirm: String::new(),
            epoch: 0,
            key: Secret::default(),
            wallet: None,
        }
    }

    /// Whether letters go into a field (so `q` and `w` are text, not commands).
    pub fn types_text(&self, mainnet: bool) -> bool {
        match self.stage {
            SendStage::Compose | SendStage::Key { .. } => true,
            SendStage::Confirm { .. } => mainnet,
            SendStage::Review { .. } | SendStage::Done => false,
        }
    }

    fn focused_text(&mut self) -> Option<&mut String> {
        match self.stage {
            SendStage::Compose => Some(match self.focus {
                FIELD_TO => &mut self.to,
                FIELD_AMOUNT => &mut self.amount,
                _ => &mut self.fee,
            }),
            SendStage::Confirm { .. } => Some(&mut self.confirm),
            _ => None,
        }
    }

    pub fn push(&mut self, c: char) {
        if matches!(self.stage, SendStage::Key { .. }) {
            self.key.push(c);
            return;
        }
        let focus = self.focus;
        let in_compose = self.stage == SendStage::Compose;
        let allowed = match (in_compose, focus) {
            (true, FIELD_AMOUNT) => c.is_ascii_digit() || c == '_',
            (true, FIELD_FEE) => c.is_ascii_digit() || c == '.',
            _ => !c.is_control(),
        };
        if allowed {
            if let Some(text) = self.focused_text() {
                text.push(c);
            }
        }
    }

    pub fn pop(&mut self) {
        if matches!(self.stage, SendStage::Key { .. }) {
            self.key.pop();
        } else if let Some(text) = self.focused_text() {
            text.pop();
        }
    }

    pub fn clear(&mut self) {
        if matches!(self.stage, SendStage::Key { .. }) {
            self.key.clear();
        } else if let Some(text) = self.focused_text() {
            text.clear();
        }
    }

    pub fn next_field(&mut self) {
        if self.stage == SendStage::Compose {
            self.focus = (self.focus + 1) % FIELDS;
        }
    }

    pub fn prev_field(&mut self) {
        if self.stage == SendStage::Compose {
            self.focus = (self.focus + FIELDS - 1) % FIELDS;
        }
    }

    /// Back to a blank form; the key and any signed transaction are dropped.
    pub fn reset(&mut self) {
        let epoch = self.epoch + 1;
        *self = SendForm::new();
        self.epoch = epoch;
    }

    /// Back to editing, keeping what was typed. Drops the key and anything built from it.
    pub fn edit_again(&mut self) {
        self.epoch += 1;
        self.stage = SendStage::Compose;
        self.summary.clear();
        self.confirm.clear();
        self.key.clear();
    }
}

impl Default for SendForm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_only_take_what_they_can_hold() {
        let mut f = SendForm::new();
        f.focus = FIELD_AMOUNT;
        for c in "12a.3_4".chars() {
            f.push(c);
        }
        assert_eq!(f.amount, "123_4");
        f.focus = FIELD_FEE;
        for c in "1.5x".chars() {
            f.push(c);
        }
        assert_eq!(f.fee, "1.5");
    }

    #[test]
    fn the_key_is_never_printed_and_goes_only_to_the_key_stage() {
        let mut f = SendForm::new();
        f.stage = SendStage::Key {
            psbt: String::new(),
        };
        for c in "cSecretKey".chars() {
            f.push(c);
        }
        assert_eq!(f.key.expose(), "cSecretKey");
        assert!(!format!("{f:?}").contains("cSecretKey"));
        assert!(!serde_json::to_string(&f).unwrap().contains("cSecretKey"));
        f.edit_again();
        assert_eq!(f.key.len(), 0);
    }
}
