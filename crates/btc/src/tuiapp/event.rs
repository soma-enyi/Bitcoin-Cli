use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use btc_core::keys::KeyType;

use crate::app::{self, AppError, Context};
use crate::output::Render;
use crate::tuiapp::app::{AppState, Handoff, TabState};

pub enum Action {
    /// Esc or Ctrl+C: always quits.
    Quit,
    /// The `q` key: quits only where `q` is not text being typed.
    QuitKey,
    /// Ctrl+U: empty the text field.
    ClearInput,
    NextTab,
    PrevTab,
    CharInput(char),
    Backspace,
    Enter,
    Up,
    Down,
    Tab,
    ShiftTab,
    Noop,
}

pub fn handle_key_event(key: KeyEvent) -> Action {
    // Raw mode turns Ctrl+C into an ordinary key, so quit on it explicitly.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
        return Action::ClearInput;
    }
    match key.code {
        KeyCode::Esc => Action::Quit,
        KeyCode::Char('q') => Action::QuitKey,
        KeyCode::Char(c) => Action::CharInput(c),
        KeyCode::Right => Action::NextTab,
        KeyCode::Left => Action::PrevTab,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::Tab => Action::Tab,
        KeyCode::BackTab => Action::ShiftTab,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Enter => Action::Enter,
        _ => Action::Noop,
    }
}

pub fn poll_event() -> Option<Action> {
    if event::poll(std::time::Duration::from_millis(100)).ok()? {
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind == KeyEventKind::Press {
                return Some(handle_key_event(key));
            }
        }
    }
    Some(Action::Noop)
}

/// Applies a key to the UI state. Returns the tab to execute when Enter starts a new job.
pub fn handle_input(app: &mut AppState, action: Action) -> Option<TabState> {
    match action {
        Action::CharInput(c) => {
            if !app.input_mode {
                return None;
            }
            match &mut app.tab_state {
                TabState::KeysAndMnemonic { key_type_selected, state: _ } => {
                    if c == ' ' {
                        *key_type_selected = !*key_type_selected;
                    }
                }
                TabState::Derive { xprv_input, .. } => xprv_input.push(c),
                TabState::Addresses { pubkey_input, .. } => pubkey_input.push(c),
                TabState::TxDecoder { hex_input, .. } => hex_input.push(c),
                TabState::BlockExplorer { height_input, .. } => height_input.push(c),
                _ => {}
            }
        }
        Action::ClearInput => {
            match &mut app.tab_state {
                TabState::Derive { xprv_input, .. } => xprv_input.clear(),
                TabState::Addresses { pubkey_input, .. } => pubkey_input.clear(),
                TabState::TxDecoder { hex_input, .. } => hex_input.clear(),
                TabState::BlockExplorer { height_input, .. } => height_input.clear(),
                _ => {}
            }
        }
        Action::Backspace => {
            match &mut app.tab_state {
                TabState::Derive { xprv_input, .. } => {
                    xprv_input.pop();
                }
                TabState::Addresses { pubkey_input, .. } => {
                    pubkey_input.pop();
                }
                TabState::TxDecoder { hex_input, .. } => {
                    hex_input.pop();
                }
                TabState::BlockExplorer { height_input, .. } => {
                    height_input.pop();
                }
                _ => {}
            }
        }
        Action::Up => {
            if let TabState::Fees { target_blocks, .. } = &mut app.tab_state {
                if *target_blocks < 100 {
                    *target_blocks += 1;
                }
            }
        }
        Action::Down => {
            if let TabState::Fees { target_blocks, .. } = &mut app.tab_state {
                if *target_blocks > 1 {
                    *target_blocks -= 1;
                }
            }
        }
        Action::Enter => {
            if app.is_loading() {
                return None;
            }
            app.start_loading();
            return Some(app.tab_state.clone());
        }
        _ => {}
    }
    None
}

/// What a finished job produces: the text to show, and maybe a value for another tab.
pub struct JobOutput {
    pub text: String,
    pub handoff: Option<Handoff>,
}

fn plain(text: String) -> Result<JobOutput, AppError> {
    Ok(JobOutput { text, handoff: None })
}

pub fn run_job(tab: &TabState, ctx: &Context) -> Result<JobOutput, AppError> {
    match tab {
        TabState::KeysAndMnemonic { key_type_selected, .. } => {
            if *key_type_selected {
                let info = app::mnemonic::new(ctx, 12, None)?;
                let xprv = info.root.xprv.to_string();
                Ok(JobOutput {
                    text: format!("{}\n\n-> root xprv sent to the Derive tab", info.render_human()),
                    handoff: Some(Handoff::Xprv(xprv)),
                })
            } else {
                let info = app::keys::generate(ctx, KeyType::Ecdsa);
                let pubkey = info.public_key.clone();
                Ok(JobOutput {
                    text: format!("{}\n\n-> public key sent to the Addresses tab", info.render_human()),
                    handoff: Some(Handoff::Pubkey(pubkey)),
                })
            }
        }
        TabState::Derive { xprv_input, path_input, count, .. } => {
            let derivation =
                app::derive::derive(ctx, xprv_input.trim(), path_input.trim(), *count as u32, None)?;
            let first_pubkey = derivation.children.first().map(|c| c.public_key.clone());
            let mut text = derivation.render_human();
            if first_pubkey.is_some() {
                text.push_str("\n\n-> first public key sent to the Addresses tab");
            }
            Ok(JobOutput { text, handoff: first_pubkey.map(Handoff::Pubkey) })
        }
        TabState::Addresses { pubkey_input, .. } => {
            app::validate::pubkey_hex(pubkey_input.trim())?;
            plain(app::address::from_pubkey(ctx, pubkey_input.trim(), None)?.render_human())
        }
        TabState::TxDecoder { hex_input, .. } => {
            plain(app::tx::decode(ctx, hex_input.trim(), app::tx::PrevoutMode::BestEffort)?.render_human())
        }
        TabState::BlockExplorer { height_input, .. } => {
            plain(app::node::block_info(ctx, height_input.trim())?.render_human())
        }
        TabState::Fees { target_blocks, .. } => {
            plain(app::node::fee_estimate(ctx, Some(*target_blocks as u16), None, None)?.render_human())
        }
        TabState::NodeStatus { .. } => plain(app::node::status(ctx)?.render_human()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use btc_core::Network;
    use btc_node::{CoreRpcBackend, MockBackend, NodeBackend, RpcConfig, RpcOptions};

    use super::*;
    use crate::output::OutputMode;
    use crate::tuiapp::app::{OperationState, Tab};

    fn ctx_with(backend: Arc<dyn NodeBackend>, rpc: RpcOptions) -> Context {
        Context {
            network: Network::Regtest,
            output: OutputMode::Human,
            rpc: RpcConfig::resolve(Network::Regtest, rpc),
            backend: Some(backend),
            fees: crate::app::FeeConfig::default(),
        }
    }

    fn mock_ctx() -> Context {
        ctx_with(Arc::new(MockBackend), RpcOptions::default())
    }

    fn unreachable_ctx() -> Context {
        let options = || RpcOptions {
            cookie: Some("/nonexistent/.cookie".into()),
            ..RpcOptions::default()
        };
        let backend = CoreRpcBackend::new(&RpcConfig::resolve(Network::Regtest, options())).unwrap();
        ctx_with(Arc::new(backend), options())
    }

    fn tab(t: Tab) -> TabState {
        TabState::new(t)
    }

    fn run_tab(tab: &TabState, ctx: &Context) -> Result<String, AppError> {
        run_job(tab, ctx).map(|out| out.text)
    }

    #[test]
    fn enter_starts_loading_and_hands_back_a_job() {
        let mut app = AppState::new();
        assert!(handle_input(&mut app, Action::Enter).is_some());
        assert!(app.is_loading());
    }

    #[test]
    fn enter_is_ignored_while_a_job_is_running() {
        let mut app = AppState::new();
        handle_input(&mut app, Action::Enter);
        assert!(handle_input(&mut app, Action::Enter).is_none());
    }

    #[test]
    fn results_replace_the_loading_state() {
        let mut app = AppState::new();
        handle_input(&mut app, Action::Enter);
        app.apply_outcome(Tab::KeysAndMnemonic, Err("boom".into()));
        assert!(!app.is_loading());
        app.apply_outcome(Tab::KeysAndMnemonic, Ok("ok".into()));
        assert!(matches!(
            &app.tab_state,
            TabState::KeysAndMnemonic { state: OperationState::Success(s), .. } if s == "ok"
        ));
    }

    #[test]
    fn keys_tab_generates_a_real_key() {
        let out = run_tab(&tab(Tab::KeysAndMnemonic), &mock_ctx()).unwrap();
        assert!(out.contains("private key"), "{out}");
    }

    #[test]
    fn derive_rejects_an_empty_or_bad_key() {
        let err = run_tab(&tab(Tab::Derive), &mock_ctx()).unwrap_err();
        assert!(!err.to_string().is_empty());
        let mut state = tab(Tab::Derive);
        if let TabState::Derive { xprv_input, .. } = &mut state {
            *xprv_input = "xprv-not-real".into();
        }
        assert!(run_tab(&state, &mock_ctx()).is_err());
    }

    #[test]
    fn addresses_rejects_a_bad_pubkey_with_a_reason() {
        let mut state = tab(Tab::Addresses);
        if let TabState::Addresses { pubkey_input, .. } = &mut state {
            *pubkey_input = "02ab".into();
        }
        let err = run_tab(&state, &mock_ctx()).unwrap_err().to_string();
        assert!(err.contains("public key must be 66 chars"), "{err}");
    }

    #[test]
    fn tx_decoder_rejects_odd_length_hex() {
        let mut state = tab(Tab::TxDecoder);
        if let TabState::TxDecoder { hex_input, .. } = &mut state {
            *hex_input = "abc".into();
        }
        let err = run_tab(&state, &mock_ctx()).unwrap_err().to_string();
        assert!(err.contains("not a valid raw transaction"), "{err}");
    }

    #[test]
    fn block_explorer_rejects_a_bad_reference() {
        let mut state = tab(Tab::BlockExplorer);
        if let TabState::BlockExplorer { height_input, .. } = &mut state {
            *height_input = "12ab".into();
        }
        let err = run_tab(&state, &mock_ctx()).unwrap_err().to_string();
        assert!(err.contains("block hash"), "{err}");
    }

    #[test]
    fn node_tab_shows_backend_data() {
        let out = run_tab(&tab(Tab::NodeStatus), &mock_ctx()).unwrap();
        assert!(out.contains("regtest") && out.contains("101"), "{out}");
    }

    #[test]
    fn node_tabs_report_an_unreachable_node() {
        for t in [Tab::NodeStatus, Tab::Fees] {
            let err = run_tab(&tab(t), &unreachable_ctx()).unwrap_err().to_string();
            assert!(err.contains("could not reach node"), "{err}");
        }
        let mut state = tab(Tab::BlockExplorer);
        if let TabState::BlockExplorer { height_input, .. } = &mut state {
            *height_input = "1".into();
        }
        let err = run_tab(&state, &unreachable_ctx()).unwrap_err().to_string();
        assert!(err.contains("could not reach node"), "{err}");
    }

    #[test]
    fn leaving_a_tab_keeps_its_input_and_output() {
        let mut app = AppState::new();
        app.apply_outcome(Tab::KeysAndMnemonic, Ok("my generated key".into()));
        app.next_tab();
        if let TabState::Derive { xprv_input, .. } = &mut app.tab_state {
            xprv_input.push_str("typed text");
        }
        app.next_tab();
        app.prev_tab();
        assert!(matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if xprv_input == "typed text"));
        app.prev_tab();
        assert!(matches!(
            &app.tab_state,
            TabState::KeysAndMnemonic { state: OperationState::Success(s), .. } if s == "my generated key"
        ));
    }

    #[test]
    fn running_again_replaces_the_kept_output() {
        let mut app = AppState::new();
        app.apply_outcome(Tab::KeysAndMnemonic, Ok("old".into()));
        app.next_tab();
        app.prev_tab();
        assert!(handle_input(&mut app, Action::Enter).is_some());
        assert!(app.is_loading());
        app.apply_outcome(Tab::KeysAndMnemonic, Ok("new".into()));
        assert!(matches!(
            &app.tab_state,
            TabState::KeysAndMnemonic { state: OperationState::Success(s), .. } if s == "new"
        ));
    }

    #[test]
    fn a_result_that_arrives_after_switching_tabs_is_not_lost() {
        let mut app = AppState::new();
        handle_input(&mut app, Action::Enter);
        app.next_tab();
        app.apply_outcome(Tab::KeysAndMnemonic, Ok("late result".into()));
        assert!(matches!(&app.tab_state, TabState::Derive { state: OperationState::Idle, .. }));
        app.prev_tab();
        assert!(matches!(
            &app.tab_state,
            TabState::KeysAndMnemonic { state: OperationState::Success(s), .. } if s == "late result"
        ));
    }

    #[test]
    fn a_generated_mnemonic_pre_fills_the_derive_tab_with_its_xprv() {
        let ctx = mock_ctx();
        let mut keys = tab(Tab::KeysAndMnemonic);
        if let TabState::KeysAndMnemonic { key_type_selected, .. } = &mut keys {
            *key_type_selected = true;
        }
        let out = run_job(&keys, &ctx).unwrap();
        assert!(out.text.contains("sent to the Derive tab"));
        let Some(Handoff::Xprv(xprv)) = out.handoff else { panic!("expected an xprv hand-off") };
        assert!(xprv.starts_with("tprv"), "{xprv}");

        let mut app = AppState::new();
        app.apply_handoff(Handoff::Xprv(xprv.clone()));
        app.next_tab();
        assert!(matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if *xprv_input == xprv));
        // ...and the Derive tab can actually use it.
        assert!(run_job(&app.tab_state, &ctx).is_ok());
    }

    #[test]
    fn a_generated_key_and_a_derived_key_pre_fill_the_addresses_tab() {
        let ctx = mock_ctx();
        let out = run_job(&tab(Tab::KeysAndMnemonic), &ctx).unwrap();
        let Some(Handoff::Pubkey(pubkey)) = out.handoff else { panic!("expected a pubkey hand-off") };
        let mut app = AppState::new();
        app.apply_handoff(Handoff::Pubkey(pubkey.clone()));
        app.next_tab();
        app.next_tab();
        assert!(matches!(&app.tab_state, TabState::Addresses { pubkey_input, .. } if *pubkey_input == pubkey));
        assert!(run_job(&app.tab_state, &ctx).is_ok(), "the hand-off must be a valid public key");
    }

    #[test]
    fn the_derive_tab_defaults_to_a_segwit_path_for_the_network() {
        let mut app = AppState::new();
        app.configure_network("MAINNET (real funds)".into(), true);
        app.next_tab();
        assert!(matches!(&app.tab_state, TabState::Derive { path_input, .. } if path_input == "m/84'/0'/0'/0/0"));
        let mut app = AppState::new();
        app.configure_network("regtest".into(), false);
        app.next_tab();
        assert!(matches!(&app.tab_state, TabState::Derive { path_input, .. } if path_input == "m/84'/1'/0'/0/0"));
    }

    fn press(code: KeyCode) -> Action {
        handle_key_event(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn q_is_typed_in_text_tabs_and_quits_elsewhere() {
        assert!(matches!(press(KeyCode::Char('q')), Action::QuitKey));
        let mut app = AppState::new();
        assert!(!app.is_text_tab(), "the Keys tab has no text field");
        app.next_tab();
        assert!(app.is_text_tab());
        handle_input(&mut app, Action::CharInput('q'));
        assert!(matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if xprv_input == "q"));
    }

    #[test]
    fn esc_and_ctrl_c_always_quit() {
        assert!(matches!(press(KeyCode::Esc), Action::Quit));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(handle_key_event(ctrl_c), Action::Quit));
        assert!(matches!(press(KeyCode::Char('c')), Action::CharInput('c')));
    }

    #[test]
    fn ctrl_u_empties_the_text_field_and_leaves_the_result() {
        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert!(matches!(handle_key_event(ctrl_u), Action::ClearInput));
        let mut app = AppState::new();
        for _ in 0..3 {
            app.next_tab();
        }
        for c in "0100000001abcdef".chars() {
            handle_input(&mut app, Action::CharInput(c));
        }
        app.apply_outcome(Tab::TxDecoder, Ok("previous result".into()));
        handle_input(&mut app, Action::ClearInput);
        assert!(matches!(
            &app.tab_state,
            TabState::TxDecoder { hex_input, state: OperationState::Success(_) } if hex_input.is_empty()
        ));
    }
}
