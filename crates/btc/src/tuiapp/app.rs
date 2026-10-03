use std::collections::HashMap;

use std::path::PathBuf;

use crate::app::wallets::Wallets;
use crate::output::Render;
use crate::tuiapp::clipboard::{Clip, SystemClip};
use crate::tuiapp::send::{SendForm, SendStage};
use crate::tuiapp::wallet_modal::WalletModal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationState {
    Idle,
    Loading,
    Success(String),
    Error(String),
}

impl OperationState {
    pub fn is_loading(&self) -> bool {
        matches!(self, OperationState::Loading)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    KeysAndMnemonic,
    Derive,
    Addresses,
    TxDecoder,
    BlockExplorer,
    Fees,
    Send,
    NodeStatus,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[
            Tab::KeysAndMnemonic,
            Tab::Derive,
            Tab::Addresses,
            Tab::TxDecoder,
            Tab::BlockExplorer,
            Tab::Fees,
            Tab::Send,
            Tab::NodeStatus,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tab::KeysAndMnemonic => "Keys & Mnemonic",
            Tab::Derive => "Derive",
            Tab::Addresses => "Addresses",
            Tab::TxDecoder => "Tx Decoder",
            Tab::BlockExplorer => "Blocks",
            Tab::Fees => "Fees",
            Tab::Send => "Send",
            Tab::NodeStatus => "Node",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum TabState {
    KeysAndMnemonic {
        key_type_selected: bool,
        state: OperationState,
    },
    Derive {
        xprv_input: String,
        path_input: String,
        count: usize,
        state: OperationState,
    },
    Addresses {
        pubkey_input: String,
        addr_type: usize,
        state: OperationState,
    },
    TxDecoder {
        hex_input: String,
        state: OperationState,
    },
    BlockExplorer {
        height_input: String,
        state: OperationState,
    },
    Fees {
        target_blocks: usize,
        /// Optional last block of the window: estimate from what blocks paid. Empty uses the node.
        height_input: String,
        /// Optional number of blocks to read; empty picks one from the target.
        blocks_input: String,
        /// Which of the two fields takes digits (Tab switches): false = height, true = blocks.
        blocks_focused: bool,
        state: OperationState,
    },
    Send {
        form: SendForm,
        state: OperationState,
    },
    NodeStatus {
        state: OperationState,
    },
}

/// A value one tab passes to another so the user never has to copy and paste it.
#[derive(Debug, Clone, PartialEq)]
pub enum Handoff {
    /// A generated root extended key, for the Derive tab.
    Xprv(String),
    /// A public key, for the Addresses tab.
    Pubkey(String),
    /// The Send tab built an unsigned payment.
    SendReview { psbt: String, summary: String },
    /// The Send tab signed it.
    SendSigned { hex: String, txid: String },
    /// The Send tab broadcast it.
    SendDone,
}

impl TabState {
    pub fn op(&self) -> &OperationState {
        match self {
            TabState::KeysAndMnemonic { state, .. }
            | TabState::Derive { state, .. }
            | TabState::Addresses { state, .. }
            | TabState::TxDecoder { state, .. }
            | TabState::BlockExplorer { state, .. }
            | TabState::Fees { state, .. }
            | TabState::Send { state, .. }
            | TabState::NodeStatus { state } => state,
        }
    }

    pub fn op_mut(&mut self) -> &mut OperationState {
        match self {
            TabState::KeysAndMnemonic { state, .. }
            | TabState::Derive { state, .. }
            | TabState::Addresses { state, .. }
            | TabState::TxDecoder { state, .. }
            | TabState::BlockExplorer { state, .. }
            | TabState::Fees { state, .. }
            | TabState::Send { state, .. }
            | TabState::NodeStatus { state } => state,
        }
    }

    pub fn new(tab: Tab) -> Self {
        match tab {
            Tab::KeysAndMnemonic => TabState::KeysAndMnemonic {
                key_type_selected: false,
                state: OperationState::Idle,
            },
            Tab::Derive => TabState::Derive {
                xprv_input: String::new(),
                path_input: "m/44'/0'/0'/0/0".to_string(),
                count: 1,
                state: OperationState::Idle,
            },
            Tab::Addresses => TabState::Addresses {
                pubkey_input: String::new(),
                addr_type: 0,
                state: OperationState::Idle,
            },
            Tab::TxDecoder => TabState::TxDecoder {
                hex_input: String::new(),
                state: OperationState::Idle,
            },
            Tab::BlockExplorer => TabState::BlockExplorer {
                height_input: String::new(),
                state: OperationState::Idle,
            },
            Tab::Fees => TabState::Fees {
                target_blocks: 6,
                height_input: String::new(),
                blocks_input: String::new(),
                blocks_focused: false,
                state: OperationState::Idle,
            },
            Tab::Send => TabState::Send {
                form: SendForm::new(),
                state: OperationState::Idle,
            },
            Tab::NodeStatus => TabState::NodeStatus {
                state: OperationState::Idle,
            },
        }
    }
}

/// A short message under the tabs.
#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
    /// `None` stays until replaced (for work in progress).
    pub until: Option<std::time::Instant>,
}

#[derive(Serialize)]
pub struct AppState {
    pub current_tab: Tab,
    pub running: bool,
    pub tab_state: TabState,
    pub input_mode: bool,
    pub frame_count: u32,
    pub network_label: String,
    pub mainnet: bool,
    /// The named wallets, saved to disk on every change.
    #[serde(skip)]
    pub wallets: Option<Wallets>,
    /// The wallet dialog, when open.
    #[serde(skip)]
    pub modal: Option<WalletModal>,
    pub network: btc_core::Network,
    /// A just-generated phrase on its way from wallet creation to the dialog.
    #[serde(skip)]
    pub(crate) pending_phrase: Option<crate::tuiapp::send::Secret>,
    #[serde(skip)]
    pub(crate) clip: Box<dyn Clip>,
    /// A one-line message for the footer (a mining result, say) and when it stops being shown.
    #[serde(skip)]
    pub status: Option<Status>,
    /// Set by `m`: mine blocks to this wallet. Taken by the main loop, which runs the job.
    #[serde(skip)]
    pub(crate) mine_request: Option<crate::app::wallets::Wallet>,
    #[serde(skip)]
    pub(crate) mining: bool,
    /// When the clipboard must be emptied, if the recovery phrase was copied to it.
    #[serde(skip)]
    pub(crate) clip_expiry: Option<std::time::Instant>,
    /// Where the session (tab and non-secret form fields) is saved on quit.
    #[serde(skip)]
    pub session_path: Option<PathBuf>,
    /// BIP84 (native segwit), the address type `tx sign` can spend. Coin type follows the network.
    pub derive_path: String,
    /// Tabs the user has left. Their inputs and results are kept until they run again.
    #[serde(skip)]
    saved: HashMap<Tab, TabState>,
}

impl AppState {
    pub fn new() -> Self {
        let tab = Tab::KeysAndMnemonic;
        Self {
            current_tab: tab,
            running: true,
            tab_state: TabState::new(tab),
            input_mode: true,
            frame_count: 0,
            network_label: String::new(),
            mainnet: false,
            wallets: None,
            modal: None,
            network: btc_core::Network::Regtest,
            pending_phrase: None,
            clip: Box::new(SystemClip::default()),
            clip_expiry: None,
            status: None,
            mine_request: None,
            mining: false,
            session_path: None,
            derive_path: "m/84'/1'/0'/0/0".to_owned(),
            saved: HashMap::new(),
        }
    }

    fn fresh(&self, tab: Tab) -> TabState {
        fresh_state(&self.derive_path, tab)
    }

    /// Sets the network-dependent defaults, then rebuilds the first tab.
    pub fn configure_network(&mut self, label: String, mainnet: bool) {
        self.network_label = label;
        self.mainnet = mainnet;
        self.derive_path = format!("m/84'/{}'/0'/0/0", if mainnet { 0 } else { 1 });
        self.tab_state = self.fresh(self.current_tab);
    }

    pub fn increment_frame(&mut self) {
        self.frame_count = self.frame_count.wrapping_add(1);
    }

    pub fn is_loading(&self) -> bool {
        self.tab_state.op().is_loading()
    }

    /// Tabs with a text field, where every character, including `q`, is input.
    pub fn is_text_tab(&self) -> bool {
        matches!(
            self.tab_state,
            TabState::Derive { .. }
                | TabState::Addresses { .. }
                | TabState::TxDecoder { .. }
                | TabState::BlockExplorer { .. }
        ) || self.modal.is_some()
            || matches!(&self.tab_state, TabState::Send { form, .. } if form.types_text(self.mainnet))
    }

    /// `m`: mine blocks to the current wallet. Regtest only; the work happens in the main loop.
    pub fn request_mine(&mut self) {
        let say = |text: &str, is_error: bool| Status {
            text: text.to_owned(),
            is_error,
            until: Some(std::time::Instant::now() + std::time::Duration::from_secs(8)),
        };
        if self.network != btc_core::Network::Regtest {
            self.status = Some(say("Mining only works on regtest.", true));
        } else if self.mining {
            self.status = Some(say("Already mining: wait for it to finish.", true));
        } else if let Some(wallet) = self.wallets.as_ref().and_then(|w| w.current().cloned()) {
            self.status = Some(Status {
                text: format!("⛏ Mining to `{}`...", wallet.name),
                is_error: false,
                until: None,
            });
            self.mining = true;
            self.mine_request = Some(wallet);
        } else {
            self.status = Some(say("Create a wallet first: press w, then n.", true));
        }
    }

    /// The mining job finished.
    pub fn finish_mining(&mut self, result: Result<String, String>) {
        self.mining = false;
        let until = Some(std::time::Instant::now() + std::time::Duration::from_secs(12));
        self.status = Some(match result {
            Ok(text) => Status {
                text,
                is_error: false,
                until,
            },
            Err(text) => Status {
                text,
                is_error: true,
                until,
            },
        });
    }

    /// The current wallet's name, for the footer.
    pub fn wallet_label(&self) -> String {
        match self.wallets.as_ref().and_then(|w| w.current()) {
            Some(w) => w.name.clone(),
            None => "none (press w)".to_owned(),
        }
    }

    pub fn start_loading(&mut self) {
        *self.tab_state.op_mut() = OperationState::Loading;
    }

    /// The state of any tab: the visible one, or the copy saved when the user left it.
    fn state_of(&mut self, tab: Tab) -> &mut TabState {
        if tab == self.current_tab {
            &mut self.tab_state
        } else {
            self.saved
                .entry(tab)
                .or_insert_with(|| fresh_state(&self.derive_path, tab))
        }
    }

    /// Records a finished job. It may belong to a tab the user has since left.
    pub fn apply_outcome(&mut self, tab: Tab, outcome: Result<String, String>) {
        let state = match outcome {
            Ok(text) => OperationState::Success(text),
            Err(err) => OperationState::Error(err),
        };
        *self.state_of(tab).op_mut() = state;
    }

    /// Pre-fills the tab a value is meant for.
    pub fn apply_handoff(&mut self, handoff: Handoff) {
        match handoff {
            Handoff::Xprv(key) => {
                if let TabState::Derive {
                    xprv_input, state, ..
                } = self.state_of(Tab::Derive)
                {
                    *xprv_input = key;
                    *state = OperationState::Idle;
                }
            }
            Handoff::SendReview { psbt, summary } => {
                if let TabState::Send { form, .. } = self.state_of(Tab::Send) {
                    form.summary = summary;
                    form.stage = SendStage::Review { psbt };
                }
            }
            Handoff::SendSigned { hex, txid } => {
                if let TabState::Send { form, .. } = self.state_of(Tab::Send) {
                    form.key.clear();
                    form.stage = SendStage::Confirm { hex, txid };
                }
            }
            Handoff::SendDone => {
                if let TabState::Send { form, .. } = self.state_of(Tab::Send) {
                    form.key.clear();
                    form.stage = SendStage::Done;
                }
            }
            Handoff::Pubkey(key) => {
                if let TabState::Addresses {
                    pubkey_input,
                    state,
                    ..
                } = self.state_of(Tab::Addresses)
                {
                    *pubkey_input = key;
                    *state = OperationState::Idle;
                }
            }
        }
    }

    /// Whether a finished job belongs to a Send form that has since been edited or reset.
    pub fn is_stale(&self, tab: Tab, epoch: Option<u64>) -> bool {
        let Some(epoch) = epoch else {
            return false;
        };
        if tab != Tab::Send {
            return false;
        }
        let state = if self.current_tab == Tab::Send {
            Some(&self.tab_state)
        } else {
            self.saved.get(&Tab::Send)
        };
        matches!(state, Some(TabState::Send { form, .. }) if form.epoch != epoch)
    }

    /// Esc backs out of the key and confirm steps (dropping the key) instead of quitting.
    pub fn esc_goes_back(&self) -> bool {
        matches!(
            &self.tab_state,
            TabState::Send { form, .. }
                if matches!(form.stage, SendStage::Key { .. } | SendStage::Confirm { .. })
        )
    }

    /// A payment built for one wallet must not be signed as another: start the Send tab over.
    pub fn reset_send_flow(&mut self) {
        if let TabState::Send { form, state } = self.state_of(Tab::Send) {
            form.reset();
            *state = OperationState::Idle;
        }
    }

    pub(crate) fn saved_state(&self, tab: Tab) -> Option<&TabState> {
        self.saved.get(&tab)
    }

    /// A mutable view of any tab's state, creating it fresh if it was never opened.
    pub(crate) fn state_mut(&mut self, tab: Tab) -> &mut TabState {
        self.state_of(tab)
    }

    pub(crate) fn go_to(&mut self, tab: Tab) {
        self.switch_to(tab);
    }

    fn switch_to(&mut self, tab: Tab) {
        if tab == self.current_tab {
            return;
        }
        let incoming = match self.saved.remove(&tab) {
            Some(saved) => saved,
            None => self.fresh(tab),
        };
        let mut outgoing = std::mem::replace(&mut self.tab_state, incoming);
        // A key typed for signing must not wait around in a tab nobody is looking at.
        if let TabState::Send { form, .. } = &mut outgoing {
            form.key.clear();
        }
        self.saved.insert(self.current_tab, outgoing);
        self.current_tab = tab;
        self.input_mode = true;
    }

    pub fn next_tab(&mut self) {
        let tabs = Tab::all();
        let idx = tabs
            .iter()
            .position(|t| t == &self.current_tab)
            .unwrap_or(0);
        self.switch_to(tabs[(idx + 1) % tabs.len()]);
    }

    pub fn prev_tab(&mut self) {
        let tabs = Tab::all();
        let idx = tabs
            .iter()
            .position(|t| t == &self.current_tab)
            .unwrap_or(0);
        self.switch_to(tabs[if idx == 0 { tabs.len() - 1 } else { idx - 1 }]);
    }
}

/// A tab as the user first sees it, with the network-appropriate Derive path.
fn fresh_state(derive_path: &str, tab: Tab) -> TabState {
    let mut state = TabState::new(tab);
    if let TabState::Derive { path_input, .. } = &mut state {
        *path_input = derive_path.to_owned();
    }
    state
}

impl Render for AppState {
    fn render_human(&self) -> String {
        format!(
            "TUI Mode - Tab: {} (use ← → to navigate, q to quit)",
            self.current_tab.name()
        )
    }
}

impl Drop for AppState {
    /// Covers every way out that does not pass through `finish`: an error, a panic.
    fn drop(&mut self) {
        self.clear_clipboard();
    }
}
