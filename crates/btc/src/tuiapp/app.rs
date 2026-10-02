use std::collections::HashMap;

use crate::output::Render;
use serde::Serialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    KeysAndMnemonic,
    Derive,
    Addresses,
    TxDecoder,
    BlockExplorer,
    Fees,
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
                state: OperationState::Idle,
            },
            Tab::NodeStatus => TabState::NodeStatus {
                state: OperationState::Idle,
            },
        }
    }
}

#[derive(Serialize)]
pub struct AppState {
    pub current_tab: Tab,
    pub running: bool,
    pub tab_state: TabState,
    pub input_mode: bool,
    pub frame_count: u32,
    pub network_label: String,
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
        )
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

    fn switch_to(&mut self, tab: Tab) {
        if tab == self.current_tab {
            return;
        }
        let incoming = match self.saved.remove(&tab) {
            Some(saved) => saved,
            None => self.fresh(tab),
        };
        let outgoing = std::mem::replace(&mut self.tab_state, incoming);
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
