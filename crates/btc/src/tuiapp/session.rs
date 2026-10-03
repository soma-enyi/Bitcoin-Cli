//! What the TUI remembers between runs: the tab and the non-secret fields. Keys, extended
//! keys, PSBTs and signed transactions are never written.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tuiapp::app::{AppState, Tab, TabState};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SavedSession {
    pub tab: Option<Tab>,
    pub block_height: String,
    pub fee_target: Option<usize>,
    pub fee_height: String,
    pub fee_blocks: String,
    pub send_to: String,
    pub send_amount: String,
    pub send_fee: String,
}

impl SavedSession {
    /// A missing or unreadable file is just a fresh start.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::app::wallets::write_private(path, text.as_bytes())
    }
}

impl AppState {
    fn peek(&self, tab: Tab) -> Option<&TabState> {
        if tab == self.current_tab {
            Some(&self.tab_state)
        } else {
            self.saved_state(tab)
        }
    }

    pub fn capture_session(&self) -> SavedSession {
        let mut out = SavedSession {
            tab: Some(self.current_tab),
            ..SavedSession::default()
        };
        if let Some(TabState::BlockExplorer { height_input, .. }) = self.peek(Tab::BlockExplorer) {
            out.block_height = height_input.clone();
        }
        if let Some(TabState::Fees {
            target_blocks,
            height_input,
            blocks_input,
            ..
        }) = self.peek(Tab::Fees)
        {
            out.fee_target = Some(*target_blocks);
            out.fee_height = height_input.clone();
            out.fee_blocks = blocks_input.clone();
        }
        if let Some(TabState::Send { form, .. }) = self.peek(Tab::Send) {
            out.send_to = form.to.clone();
            out.send_amount = form.amount.clone();
            out.send_fee = form.fee.clone();
        }
        out
    }

    pub fn apply_session(&mut self, saved: SavedSession) {
        if let TabState::BlockExplorer { height_input, .. } = self.state_mut(Tab::BlockExplorer) {
            *height_input = saved.block_height;
        }
        if let TabState::Fees {
            target_blocks,
            height_input,
            blocks_input,
            ..
        } = self.state_mut(Tab::Fees)
        {
            if let Some(t) = saved.fee_target.filter(|t| (1..=100).contains(t)) {
                *target_blocks = t;
            }
            *height_input = saved.fee_height;
            *blocks_input = saved.fee_blocks;
        }
        if let TabState::Send { form, .. } = self.state_mut(Tab::Send) {
            form.to = saved.send_to;
            form.amount = saved.send_amount;
            form.fee = saved.send_fee;
        }
        if let Some(tab) = saved.tab {
            self.go_to(tab);
        }
    }

    /// Everything to do on the way out: save the session and empty the clipboard of any phrase.
    pub fn finish(&mut self) {
        self.save_session();
        self.clear_clipboard();
    }

    /// Writes the session next to the wallets.
    pub fn save_session(&self) {
        if let Some(path) = &self.session_path {
            // Quitting must not fail because the disk is full or read-only.
            let _ = self.capture_session().save(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_round_trips_and_holds_no_secrets() {
        let mut app = AppState::new();
        app.go_to(Tab::Send);
        if let TabState::Send { form, .. } = &mut app.tab_state {
            form.to = "bcrt1qexample".into();
            form.amount = "25000".into();
            form.stage = crate::tuiapp::send::SendStage::Key {
                psbt: "SECRETPSBT".into(),
            };
            for c in "cSecretKey".chars() {
                form.key.push(c);
            }
        }
        if let TabState::Derive { xprv_input, .. } = app.state_mut(Tab::Derive) {
            *xprv_input = "xprvSECRET".into();
        }

        let dir = std::env::temp_dir().join(format!("btc-session-{}", std::process::id()));
        let path = dir.join("session.json");
        app.session_path = Some(path.clone());
        app.save_session();

        let text = fs::read_to_string(&path).unwrap();
        for secret in ["cSecretKey", "SECRETPSBT", "xprvSECRET"] {
            assert!(!text.contains(secret), "{secret} was saved: {text}");
        }

        let mut fresh = AppState::new();
        fresh.apply_session(SavedSession::load(&path));
        assert_eq!(fresh.current_tab, Tab::Send);
        let TabState::Send { form, .. } = &fresh.tab_state else {
            panic!("not the send tab")
        };
        assert_eq!(
            (form.to.as_str(), form.amount.as_str()),
            ("bcrt1qexample", "25000")
        );
        assert_eq!(form.stage, crate::tuiapp::send::SendStage::Compose);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_file_is_a_fresh_start() {
        let dir = std::env::temp_dir().join(format!("btc-session-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        fs::write(&path, "not json").unwrap();
        assert_eq!(SavedSession::load(&path), SavedSession::default());
        let _ = fs::remove_dir_all(dir);
    }
}
