use btc_core::keys::KeyType;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{self, AppError, Context};
use crate::output::Render;
use crate::tuiapp::app::{AppState, Handoff, OperationState, TabState};
use crate::tuiapp::send::{SendForm, SendStage};
use crate::tuiapp::wallet_modal::ModalKey;

pub enum Action {
    /// Ctrl+C: always quits.
    Quit,
    /// Esc: closes the wallet dialog if it is open, otherwise quits.
    Esc,
    /// Ctrl+W: open the wallet dialog even from a text field.
    Wallets,
    /// Ctrl+O: mine blocks (regtest) even from a text field. Plain `m` does it elsewhere.
    Mine,
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
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
        return Action::Mine;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
        return Action::Wallets;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
        return Action::ClearInput;
    }
    match key.code {
        KeyCode::Esc => Action::Esc,
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
    if app.modal.is_some() {
        let key = match action {
            Action::CharInput(c) => Some(ModalKey::Char(c)),
            // `q` is not text in the list view, so it reaches us as QuitKey.
            Action::QuitKey => Some(ModalKey::Char('q')),
            Action::Backspace => Some(ModalKey::Backspace),
            Action::Enter => Some(ModalKey::Enter),
            Action::Up => Some(ModalKey::Up),
            Action::Down => Some(ModalKey::Down),
            Action::Tab | Action::ShiftTab => Some(ModalKey::Tab),
            Action::ClearInput => Some(ModalKey::Clear),
            Action::Esc | Action::Wallets => Some(ModalKey::Close),
            _ => None,
        };
        if let Some(key) = key {
            app.modal_key(key);
        }
        return None;
    }
    if matches!(action, Action::Wallets) {
        app.open_wallets();
        return None;
    }
    if matches!(action, Action::Mine) {
        app.request_mine();
        return None;
    }
    if let Action::CharInput(c @ ('w' | 'm')) = action {
        if !app.is_text_tab() {
            if c == 'w' {
                app.open_wallets();
            } else {
                app.request_mine();
            }
            return None;
        }
    }
    if let TabState::Send { .. } = app.tab_state {
        if let Some(handled) = send_input(app, &action) {
            return handled;
        }
    }
    match action {
        Action::CharInput(c) => {
            if !app.input_mode {
                return None;
            }
            match &mut app.tab_state {
                TabState::KeysAndMnemonic {
                    key_type_selected, ..
                } if c == ' ' => *key_type_selected = !*key_type_selected,
                TabState::Derive { xprv_input, .. } => xprv_input.push(c),
                TabState::Addresses { pubkey_input, .. } => pubkey_input.push(c),
                TabState::TxDecoder { hex_input, .. } => hex_input.push(c),
                TabState::BlockExplorer { height_input, .. } => height_input.push(c),
                TabState::Fees {
                    height_input,
                    blocks_input,
                    blocks_focused,
                    ..
                } if c.is_ascii_digit() => {
                    if *blocks_focused {
                        blocks_input.push(c)
                    } else {
                        height_input.push(c)
                    }
                }
                _ => {}
            }
        }
        Action::ClearInput => match &mut app.tab_state {
            TabState::Derive { xprv_input, .. } => xprv_input.clear(),
            TabState::Addresses { pubkey_input, .. } => pubkey_input.clear(),
            TabState::TxDecoder { hex_input, .. } => hex_input.clear(),
            TabState::BlockExplorer { height_input, .. } => height_input.clear(),
            TabState::Fees {
                height_input,
                blocks_input,
                blocks_focused,
                ..
            } => {
                if *blocks_focused {
                    blocks_input.clear()
                } else {
                    height_input.clear()
                }
            }
            _ => {}
        },
        Action::Backspace => match &mut app.tab_state {
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
            TabState::Fees {
                height_input,
                blocks_input,
                blocks_focused,
                ..
            } => {
                if *blocks_focused {
                    blocks_input.pop();
                } else {
                    height_input.pop();
                }
            }
            _ => {}
        },
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
        Action::Tab | Action::ShiftTab => {
            if let TabState::Fees { blocks_focused, .. } = &mut app.tab_state {
                *blocks_focused = !*blocks_focused;
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
    Ok(JobOutput {
        text,
        handoff: None,
    })
}

pub fn run_job(tab: &TabState, ctx: &Context) -> Result<JobOutput, AppError> {
    match tab {
        TabState::KeysAndMnemonic {
            key_type_selected, ..
        } => {
            if *key_type_selected {
                let info = app::mnemonic::new(ctx, 12, None)?;
                let xprv = info.root.xprv.to_string();
                Ok(JobOutput {
                    text: format!(
                        "{}\n\n-> root xprv sent to the Derive tab",
                        info.render_human()
                    ),
                    handoff: Some(Handoff::Xprv(xprv)),
                })
            } else {
                let info = app::keys::generate(ctx, KeyType::Ecdsa);
                let pubkey = info.public_key.clone();
                Ok(JobOutput {
                    text: format!(
                        "{}\n\n-> public key sent to the Addresses tab",
                        info.render_human()
                    ),
                    handoff: Some(Handoff::Pubkey(pubkey)),
                })
            }
        }
        TabState::Derive {
            xprv_input,
            path_input,
            count,
            ..
        } => {
            let derivation = app::derive::derive(
                ctx,
                xprv_input.trim(),
                path_input.trim(),
                *count as u32,
                None,
            )?;
            let first_pubkey = derivation.children.first().map(|c| c.public_key.clone());
            let mut text = derivation.render_human();
            if first_pubkey.is_some() {
                text.push_str("\n\n-> first public key sent to the Addresses tab");
            }
            Ok(JobOutput {
                text,
                handoff: first_pubkey.map(Handoff::Pubkey),
            })
        }
        TabState::Addresses { pubkey_input, .. } => {
            app::validate::pubkey_hex(pubkey_input.trim())?;
            plain(app::address::from_pubkey(ctx, pubkey_input.trim(), None)?.render_human())
        }
        TabState::TxDecoder { hex_input, .. } => plain(
            app::tx::decode(ctx, hex_input.trim(), app::tx::PrevoutMode::BestEffort)?
                .render_human(),
        ),
        TabState::BlockExplorer { height_input, .. } => {
            plain(app::node::block_info(ctx, height_input.trim())?.render_human())
        }
        TabState::Fees {
            target_blocks,
            height_input,
            blocks_input,
            ..
        } => {
            let target = Some(*target_blocks as u16);
            let (height, blocks) = (height_input.trim(), blocks_input.trim());
            if height.is_empty() && blocks.is_empty() {
                plain(app::node::fee_estimate(ctx, target, None, None)?.render_human())
            } else {
                let end = (!height.is_empty())
                    .then(|| app::validate::block_height(height))
                    .transpose()?;
                let count = (!blocks.is_empty())
                    .then(|| {
                        blocks.parse::<u16>().map_err(|_| {
                            AppError::Input("number of blocks must be a number up to 1008".into())
                        })
                    })
                    .transpose()?;
                plain(app::node::fee_estimate_from_blocks(ctx, end, count, target)?.render_human())
            }
        }
        TabState::Send { form, .. } => send_job(form, ctx),
        TabState::NodeStatus { .. } => plain(app::node::status(ctx)?.render_human()),
    }
}

/// Keys on the Send tab. `None` means "not mine": the shared handling takes over (tab
/// switching, up/down on other tabs). `Some(job)` is the result for the caller to return.
fn send_input(app: &mut AppState, action: &Action) -> Option<Option<TabState>> {
    let mainnet = app.mainnet;
    let wallet = app.wallets.as_ref().and_then(|w| w.current().cloned());
    let TabState::Send { form, state } = &mut app.tab_state else {
        return None;
    };
    match action {
        Action::CharInput(c) => {
            if form.types_text(mainnet) {
                form.push(*c);
            } else if *c == 'b' && matches!(form.stage, SendStage::Review { .. }) {
                form.edit_again();
                *state = OperationState::Idle;
            }
        }
        Action::QuitKey => {
            // Only reached while `q` is text (see `is_text_tab`), which the caller routes here.
            if form.types_text(mainnet) {
                form.push('q');
            }
        }
        Action::Backspace => form.pop(),
        Action::ClearInput => form.clear(),
        Action::Up => form.prev_field(),
        Action::Down => form.next_field(),
        Action::Tab => match form.stage {
            SendStage::Compose => form.next_field(),
            // Back out: drop the key (or the signed transaction) and edit again.
            SendStage::Key { .. } | SendStage::Confirm { .. } => {
                form.edit_again();
                *state = OperationState::Idle;
            }
            _ => {}
        },
        Action::ShiftTab => form.prev_field(),
        Action::Enter => return Some(send_enter(form, state, wallet, mainnet)),
        _ => return None,
    }
    Some(None)
}

fn send_enter(
    form: &mut SendForm,
    state: &mut OperationState,
    wallet: Option<crate::app::wallets::Wallet>,
    mainnet: bool,
) -> Option<TabState> {
    if state.is_loading() {
        return None;
    }
    let error = |state: &mut OperationState, msg: &str| *state = OperationState::Error(msg.into());
    match form.stage.clone() {
        SendStage::Compose => {
            let Some(wallet) = wallet else {
                error(state, "No wallet yet: press Ctrl+W to add one.");
                return None;
            };
            form.wallet = Some(wallet);
        }
        SendStage::Review { psbt } => {
            form.stage = SendStage::Key { psbt };
            *state = OperationState::Idle;
            return None;
        }
        SendStage::Key { .. } => {
            if form.key.expose().trim().is_empty() {
                error(state, "Type the private key (WIF) of this wallet.");
                return None;
            }
        }
        SendStage::Confirm { .. } => {
            if mainnet && form.confirm.trim() != "yes" {
                error(
                    state,
                    "This is MAINNET: real bitcoin, and it cannot be undone. Type `yes`, then Enter.",
                );
                return None;
            }
        }
        SendStage::Done => {
            form.reset();
            *state = OperationState::Idle;
            return None;
        }
    }
    *state = OperationState::Loading;
    Some(TabState::Send {
        form: form.clone(),
        state: OperationState::Loading,
    })
}

fn send_job(form: &SendForm, ctx: &Context) -> Result<JobOutput, AppError> {
    match &form.stage {
        SendStage::Compose => {
            let wallet = form
                .wallet
                .as_ref()
                .ok_or_else(|| AppError::Input("no wallet: press Ctrl+W to add one".into()))?;
            let amount = app::send::parse_amount_sats(&form.amount)?;
            let fee = match form.fee.trim() {
                "" => None,
                text => Some(text.parse::<f64>().map_err(|_| {
                    AppError::Input("the fee rate is a number, e.g. 2 or 1.5".into())
                })?),
            };
            let plan = app::send::prepare(ctx, wallet, form.to.trim(), amount, fee)?;
            Ok(JobOutput {
                text: format!(
                    "{}\n\nEnter: continue to signing   b: edit the payment",
                    plan.summary
                ),
                handoff: Some(Handoff::SendReview {
                    psbt: plan.psbt,
                    summary: plan.summary,
                }),
            })
        }
        SendStage::Key { psbt } => {
            let key = form.key.expose().trim();
            // `-` would make `tx sign` read stdin, which is the terminal the TUI is drawing on.
            if key == "-" {
                return Err(AppError::Input("type the key itself, not `-`".into()));
            }
            // Several words are a recovery phrase: sign with the key of its first address.
            let wif;
            let key = if key.split_whitespace().count() > 1 {
                wif = app::wallets::wif_from_phrase(ctx.network, key)?;
                wif.as_str()
            } else {
                key
            };
            let signed = app::tx::sign(ctx, psbt, Some(key), None)?;
            let decoded = app::tx::decode(ctx, &signed.hex, app::tx::PrevoutMode::Off)?;
            Ok(JobOutput {
                text: format!(
                    "Signed. txid {}\n\n{}\n\nEnter: BROADCAST   Tab: discard and edit",
                    signed.txid,
                    decoded.render_human()
                ),
                handoff: Some(Handoff::SendSigned {
                    hex: signed.hex,
                    txid: signed.txid,
                }),
            })
        }
        SendStage::Confirm { hex, .. } => {
            // The mainnet confirmation was already taken in `send_enter`.
            let sent = app::tx::broadcast(ctx, hex, true)?;
            Ok(JobOutput {
                text: format!(
                    "Broadcast. txid {}\n\nIt is in the mempool; it counts once it confirms. \
                     Enter: new payment",
                    sent.txid
                ),
                handoff: Some(Handoff::SendDone),
            })
        }
        SendStage::Review { .. } | SendStage::Done => plain(String::new()),
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
        let backend =
            CoreRpcBackend::new(&RpcConfig::resolve(Network::Regtest, options())).unwrap();
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
    fn fees_tab_estimates_from_a_typed_block_height() {
        let mut app = AppState::new();
        app.tab_state = tab(Tab::Fees);
        for c in "1x".chars() {
            handle_input(&mut app, Action::CharInput(c));
        }
        // Tab moves to the number-of-blocks field.
        handle_input(&mut app, Action::Tab);
        handle_input(&mut app, Action::CharInput('1'));
        // Only digits are accepted.
        let job = handle_input(&mut app, Action::Enter).unwrap();
        let TabState::Fees {
            height_input,
            blocks_input,
            ..
        } = &job
        else {
            panic!("not the fees tab")
        };
        assert_eq!((height_input.as_str(), blocks_input.as_str()), ("1", "1"));
        let out = run_tab(&job, &mock_ctx()).unwrap();
        assert!(out.contains("1-1"), "{out}");
    }

    fn type_text(app: &mut AppState, text: &str) {
        for c in text.chars() {
            handle_input(app, Action::CharInput(c));
        }
    }

    fn run_enter(app: &mut AppState, ctx: &Context) {
        if let Some(job) = handle_input(app, Action::Enter) {
            let tab = app.current_tab;
            match run_job(&job, ctx) {
                Ok(out) => {
                    app.apply_outcome(tab, Ok(out.text));
                    if let Some(h) = out.handoff {
                        app.apply_handoff(h);
                    }
                }
                Err(e) => app.apply_outcome(tab, Err(e.to_string())),
            }
        }
    }

    fn app_with_wallet(name: &str) -> (AppState, String, std::path::PathBuf) {
        use bitcoin::{CompressedPublicKey, KnownHrp};
        let key = btc_core::keys::generate(Network::Regtest, KeyType::Ecdsa);
        let wif = key.private_key_wif.to_string();
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let public = CompressedPublicKey(
            btc_core::keys::parse_private_key(&wif)
                .unwrap()
                .inner
                .public_key(&secp),
        );
        let address = bitcoin::Address::p2wpkh(&public, KnownHrp::Regtest).to_string();
        let dir = std::env::temp_dir().join(format!("btc-tui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = AppState::new();
        app.wallets = Some(
            crate::app::wallets::Wallets::open(&dir.join("wallets.json"), Network::Regtest)
                .unwrap(),
        );
        app.wallets
            .as_mut()
            .unwrap()
            .add("alice", &address)
            .unwrap();
        app.go_to(Tab::Send);
        (app, wif, dir)
    }

    #[test]
    fn send_flow_goes_compose_review_sign_broadcast() {
        let (mut app, wif, dir) = app_with_wallet("flow");
        let ctx = mock_ctx();
        type_text(&mut app, "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080");
        handle_input(&mut app, Action::Tab);
        type_text(&mut app, "25000");
        handle_input(&mut app, Action::Tab);
        type_text(&mut app, "2");

        run_enter(&mut app, &ctx);
        let TabState::Send { form, state } = &app.tab_state else {
            panic!()
        };
        assert!(matches!(form.stage, SendStage::Review { .. }), "{state:?}");
        // On a text-less stage `q` and `w` are commands, not letters.
        assert!(!app.is_text_tab());

        run_enter(&mut app, &ctx); // continue to the key
        type_text(&mut app, &wif);
        run_enter(&mut app, &ctx); // sign
        let TabState::Send { form, .. } = &app.tab_state else {
            panic!()
        };
        assert!(matches!(form.stage, SendStage::Confirm { .. }));
        assert_eq!(form.key.len(), 0, "the key must be wiped once used");

        run_enter(&mut app, &ctx); // broadcast (regtest: no extra confirmation)
        let TabState::Send { form, state } = &app.tab_state else {
            panic!()
        };
        assert_eq!(form.stage, SendStage::Done);
        assert!(matches!(state, OperationState::Success(t) if t.contains("Broadcast")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn send_needs_a_wallet_and_mainnet_needs_a_typed_yes() {
        let mut app = AppState::new();
        app.go_to(Tab::Send);
        handle_input(&mut app, Action::Enter);
        assert!(matches!(app.tab_state.op(), OperationState::Error(e) if e.contains("wallet")));

        let mut form = SendForm::new();
        form.stage = SendStage::Confirm {
            hex: "00".into(),
            txid: "11".into(),
        };
        let mut state = OperationState::Idle;
        assert!(send_enter(&mut form, &mut state, None, true).is_none());
        assert!(matches!(state, OperationState::Error(ref e) if e.contains("yes")));
        form.confirm = "yes".into();
        assert!(send_enter(&mut form, &mut state, None, true).is_some());
    }

    #[test]
    fn w_opens_the_wallet_dialog_and_wallets_can_be_added_and_switched() {
        let (mut app, _wif, dir) = app_with_wallet("modal");
        app.go_to(Tab::NodeStatus);
        handle_input(&mut app, Action::CharInput('w'));
        assert!(app.modal.is_some());
        // `q` closes the dialog instead of quitting.
        handle_input(&mut app, Action::CharInput('q'));
        assert!(app.modal.is_none());

        // `a` watches an address that already exists.
        handle_input(&mut app, Action::Wallets);
        handle_input(&mut app, Action::CharInput('a'));
        type_text(&mut app, "bob");
        handle_input(&mut app, Action::Enter);
        type_text(&mut app, "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080");
        handle_input(&mut app, Action::Enter);
        assert!(app.modal.is_none());
        assert_eq!(app.wallets.as_ref().unwrap().current().unwrap().name, "bob");

        // A bad address is reported and the dialog stays open.
        handle_input(&mut app, Action::Wallets);
        handle_input(&mut app, Action::CharInput('a'));
        type_text(&mut app, "carol");
        handle_input(&mut app, Action::Enter);
        type_text(&mut app, "not-an-address");
        handle_input(&mut app, Action::Enter);
        assert!(app.modal.as_ref().unwrap().error.is_some());
        handle_input(&mut app, Action::Esc);
        handle_input(&mut app, Action::Esc);
        assert!(app.modal.is_none());
        assert_eq!(app.wallets.as_ref().unwrap().list().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A clipboard that records what it was asked to do.
    #[derive(Clone, Default)]
    struct FakeClip {
        log: Arc<std::sync::Mutex<Vec<String>>>,
        broken: bool,
    }

    impl crate::tuiapp::clipboard::Clip for FakeClip {
        fn set(&mut self, text: &str) -> Result<(), String> {
            if self.broken {
                return Err("no clipboard available here".into());
            }
            self.log.lock().unwrap().push(format!("set:{text}"));
            Ok(())
        }
        fn clear(&mut self) -> Result<(), String> {
            self.log.lock().unwrap().push("clear".into());
            Ok(())
        }
    }

    fn create_dialog_app(name: &str, clip: FakeClip) -> (AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("btc-create-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = AppState::new();
        app.wallets = Some(
            crate::app::wallets::Wallets::open(&dir.join("wallets.json"), Network::Regtest)
                .unwrap(),
        );
        app.clip = Box::new(clip);
        // An empty wallet list opens straight on the create form.
        app.open_wallets();
        (app, dir)
    }

    /// Types the acknowledgement that lets the phrase dialog close.
    fn acknowledge(app: &mut AppState, now: std::time::Instant) {
        for c in "saved".chars() {
            app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Char(c), now);
        }
    }

    fn shown_phrase(app: &AppState) -> Option<String> {
        use crate::tuiapp::wallet_modal::{ModalMode, WalletModal};
        match &app.modal {
            Some(WalletModal {
                mode: ModalMode::Created { phrase, .. },
                ..
            }) => phrase.as_ref().map(|p| p.expose().to_owned()),
            _ => None,
        }
    }

    #[test]
    fn capitals_in_a_wallet_name_become_lowercase_instead_of_an_error() {
        let (mut app, dir) = create_dialog_app("caps", FakeClip::default());
        type_text(&mut app, "Xoulomon");
        handle_input(&mut app, Action::Enter);
        assert!(shown_phrase(&app).is_some());
        assert_eq!(
            app.wallets.as_ref().unwrap().current().unwrap().name,
            "xoulomon"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_wallet_defaults_to_24_words_and_the_choice_cycles() {
        use crate::tuiapp::wallet_modal::ModalMode;
        let (mut app, dir) = create_dialog_app("words", FakeClip::default());
        let words = |app: &AppState| match &app.modal.as_ref().unwrap().mode {
            ModalMode::Create { words, .. } => *words,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(words(&app), 24);
        handle_input(&mut app, Action::Down);
        assert_eq!(words(&app), 21);
        handle_input(&mut app, Action::Up);
        handle_input(&mut app, Action::Up);
        assert_eq!(words(&app), 12);

        handle_input(&mut app, Action::Down); // back to 24
        type_text(&mut app, "carol");
        handle_input(&mut app, Action::Enter);
        let phrase = shown_phrase(&app).expect("the phrase is shown");
        assert_eq!(phrase.split_whitespace().count(), 24);
        let wallets = app.wallets.as_ref().unwrap();
        assert_eq!(wallets.current().unwrap().name, "carol");
        assert!(wallets.current().unwrap().address.starts_with("bcrt1q"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn c_copies_the_phrase_and_the_clipboard_and_screen_clear_after_a_minute() {
        use std::time::{Duration, Instant};
        let clip = FakeClip::default();
        let (mut app, dir) = create_dialog_app("copy", clip.clone());
        let t0 = Instant::now();
        type_text(&mut app, "carol");
        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Enter, t0);
        let phrase = shown_phrase(&app).unwrap();

        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Char('c'), t0);
        assert_eq!(
            clip.log.lock().unwrap().as_slice(),
            [format!("set:{phrase}")]
        );
        assert!(app.modal.as_ref().unwrap().notice.is_some());

        app.tick_at(t0 + Duration::from_secs(59));
        assert_eq!(shown_phrase(&app), Some(phrase), "still there at 59s");
        assert_eq!(clip.log.lock().unwrap().len(), 1, "not cleared yet");

        app.tick_at(t0 + Duration::from_secs(61));
        assert_eq!(shown_phrase(&app), None, "wiped from the screen at 60s");
        assert_eq!(clip.log.lock().unwrap().last().unwrap(), "clear");
        // Once hidden it cannot be copied again.
        app.modal_key_at(
            crate::tuiapp::wallet_modal::ModalKey::Char('c'),
            t0 + Duration::from_secs(62),
        );
        assert_eq!(clip.log.lock().unwrap().len(), 2, "no second copy");
        // With nothing left to protect, Enter closes the dialog.
        app.modal_key_at(
            crate::tuiapp::wallet_modal::ModalKey::Enter,
            t0 + Duration::from_secs(63),
        );
        assert!(app.modal.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_clipboard_still_clears_after_the_dialog_is_closed_and_on_quit() {
        use std::time::{Duration, Instant};
        let clip = FakeClip::default();
        let (mut app, dir) = create_dialog_app("close", clip.clone());
        let t0 = Instant::now();
        type_text(&mut app, "dave");
        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Enter, t0);
        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Char('c'), t0);
        // Closing the dialog drops the phrase; the copy is still on the clipboard.
        acknowledge(&mut app, t0);
        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Enter, t0);
        assert!(app.modal.is_none());
        app.tick_at(t0 + Duration::from_secs(30));
        assert_eq!(clip.log.lock().unwrap().len(), 1);
        app.tick_at(t0 + Duration::from_secs(61));
        assert_eq!(clip.log.lock().unwrap().last().unwrap(), "clear");

        // Quitting early clears it too, but only if a phrase was copied.
        let clip2 = FakeClip::default();
        let (mut app2, dir2) = create_dialog_app("quit", clip2.clone());
        type_text(&mut app2, "erin");
        handle_input(&mut app2, Action::Enter);
        app2.finish();
        assert!(clip2.log.lock().unwrap().is_empty(), "nothing was copied");
        handle_input(&mut app2, Action::CharInput('c'));
        app2.finish();
        assert_eq!(clip2.log.lock().unwrap().last().unwrap(), "clear");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[test]
    fn the_phrase_survives_a_stray_enter_or_esc_and_closes_only_after_saved() {
        let now = std::time::Instant::now();
        let (mut app, dir) = create_dialog_app("ack", FakeClip::default());
        type_text(&mut app, "ivy");
        handle_input(&mut app, Action::Enter);
        let phrase = shown_phrase(&app).unwrap();

        // Double Enter, Esc, a wrong word: none of them throw the phrase away.
        handle_input(&mut app, Action::Enter);
        handle_input(&mut app, Action::Enter);
        handle_input(&mut app, Action::Esc);
        assert_eq!(shown_phrase(&app), Some(phrase.clone()));
        assert!(app.modal.as_ref().unwrap().error.is_some());
        type_text(&mut app, "sav");
        handle_input(&mut app, Action::Enter);
        assert_eq!(shown_phrase(&app), Some(phrase));

        // Typed letters go to the acknowledgement, never to the wallet.
        type_text(&mut app, "ed");
        app.modal_key_at(crate::tuiapp::wallet_modal::ModalKey::Enter, now);
        assert!(app.modal.is_none());
        assert_eq!(app.wallets.as_ref().unwrap().list().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn m_mines_for_the_current_wallet_on_regtest_only_and_is_text_in_a_field() {
        let (mut app, _wif, dir) = app_with_wallet("mine");
        app.go_to(Tab::NodeStatus);
        handle_input(&mut app, Action::CharInput('m'));
        assert_eq!(app.mine_request.as_ref().unwrap().name, "alice");
        assert!(app.status.as_ref().unwrap().text.contains("Mining"));

        // A second press while the first runs is refused, not queued.
        app.mine_request = None;
        handle_input(&mut app, Action::CharInput('m'));
        assert!(app.mine_request.is_none());
        assert!(app.status.as_ref().unwrap().is_error);

        // The result replaces the message and frees the next press.
        app.finish_mining(Ok("Mined 101 blocks".into()));
        assert!(!app.mining);
        assert_eq!(app.status.as_ref().unwrap().text, "Mined 101 blocks");
        // The message goes away by itself.
        app.tick_at(std::time::Instant::now() + std::time::Duration::from_secs(30));
        assert!(app.status.is_none());

        // In a text field `m` is a letter; Ctrl+O mines.
        app.go_to(Tab::Derive);
        handle_input(&mut app, Action::CharInput('m'));
        assert!(app.mine_request.is_none());
        assert!(matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if xprv_input == "m"));
        handle_input(&mut app, Action::Mine);
        assert!(app.mine_request.is_some());
        let ctrl_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
        assert!(matches!(handle_key_event(ctrl_o), Action::Mine));

        // Other networks refuse, and a wallet is needed.
        let mut other = AppState::new();
        other.go_to(Tab::NodeStatus);
        handle_input(&mut other, Action::CharInput('m'));
        assert!(other.status.as_ref().unwrap().text.contains("wallet"));
        other.network = Network::Signet;
        handle_input(&mut other, Action::CharInput('m'));
        assert!(other.status.as_ref().unwrap().text.contains("regtest"));
        assert!(other.mine_request.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_late_result_cannot_land_in_a_payment_that_was_edited_or_reset() {
        let (mut app, _wif, dir) = app_with_wallet("stale");
        let TabState::Send { form, .. } = &app.tab_state else {
            panic!()
        };
        let started_at = form.epoch;
        assert!(!app.is_stale(Tab::Send, Some(started_at)));

        // Switching wallet resets the payment, so the old job's result is stale.
        app.reset_send_flow();
        assert!(app.is_stale(Tab::Send, Some(started_at)));
        // Also while the user is on another tab.
        let epoch = match &app.tab_state {
            TabState::Send { form, .. } => form.epoch,
            _ => panic!(),
        };
        app.go_to(Tab::Fees);
        assert!(!app.is_stale(Tab::Send, Some(epoch)));
        app.reset_send_flow();
        assert!(app.is_stale(Tab::Send, Some(epoch)));
        // Other tabs never go stale.
        assert!(!app.is_stale(Tab::Fees, None));

        // Backing out of the key stage ("discard") also invalidates a signing job in flight.
        app.go_to(Tab::Send);
        if let TabState::Send { form, .. } = &mut app.tab_state {
            form.stage = SendStage::Key { psbt: "p".into() };
        }
        let before = match &app.tab_state {
            TabState::Send { form, .. } => form.epoch,
            _ => panic!(),
        };
        handle_input(&mut app, Action::Tab);
        assert!(app.is_stale(Tab::Send, Some(before)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn esc_backs_out_of_the_key_step_and_a_typed_key_is_dropped_on_leaving_the_tab() {
        let (mut app, _wif, dir) = app_with_wallet("esc");
        assert!(!app.esc_goes_back(), "Esc quits from the compose form");
        if let TabState::Send { form, .. } = &mut app.tab_state {
            form.stage = SendStage::Key { psbt: "p".into() };
        }
        assert!(app.esc_goes_back());
        type_text(&mut app, "cSomethingSecret");
        app.go_to(Tab::Fees);
        app.go_to(Tab::Send);
        let TabState::Send { form, .. } = &app.tab_state else {
            panic!()
        };
        assert_eq!(form.key.len(), 0, "the key must not wait in a hidden tab");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dropping_the_app_empties_a_clipboard_it_filled() {
        let clip = FakeClip::default();
        {
            let (mut app, dir) = create_dialog_app("drop", clip.clone());
            type_text(&mut app, "jade");
            handle_input(&mut app, Action::Enter);
            handle_input(&mut app, Action::CharInput('c'));
            let _ = std::fs::remove_dir_all(dir);
        }
        assert_eq!(clip.log.lock().unwrap().last().unwrap(), "clear");
    }

    #[test]
    fn a_missing_clipboard_is_reported_and_nothing_is_scheduled() {
        let clip = FakeClip {
            broken: true,
            ..FakeClip::default()
        };
        let (mut app, dir) = create_dialog_app("broken", clip);
        type_text(&mut app, "frank");
        handle_input(&mut app, Action::Enter);
        handle_input(&mut app, Action::CharInput('c'));
        let error = app.modal.as_ref().unwrap().error.clone().unwrap();
        assert!(error.contains("no clipboard"), "{error}");
        assert!(app.clip_expiry.is_none());
        assert!(shown_phrase(&app).is_some(), "the phrase stays on screen");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_phrase_is_never_written_to_disk() {
        let (mut app, dir) = create_dialog_app("disk", FakeClip::default());
        type_text(&mut app, "gina");
        handle_input(&mut app, Action::Enter);
        let phrase = shown_phrase(&app).unwrap();
        app.session_path = Some(dir.join("session.json"));
        app.finish();
        for file in ["wallets.json", "session.json"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            for word in phrase.split_whitespace().take(3) {
                assert!(
                    !text.contains(&format!("\"{word} ")),
                    "{file} holds the phrase"
                );
            }
            assert!(!text.contains(&phrase), "{file} holds the phrase");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wallet_created_in_the_dialog_can_pay_by_typing_its_phrase() {
        let (mut app, dir) = create_dialog_app("pay", FakeClip::default());
        let ctx = mock_ctx();
        type_text(&mut app, "hana");
        handle_input(&mut app, Action::Enter);
        let phrase = shown_phrase(&app).unwrap();
        acknowledge(&mut app, std::time::Instant::now());
        handle_input(&mut app, Action::Enter); // close the dialog
        assert!(app.modal.is_none());

        app.go_to(Tab::Send);
        type_text(&mut app, "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080");
        handle_input(&mut app, Action::Tab);
        type_text(&mut app, "25000");
        handle_input(&mut app, Action::Tab);
        type_text(&mut app, "2");
        run_enter(&mut app, &ctx); // review
        run_enter(&mut app, &ctx); // key stage
        type_text(&mut app, &phrase);
        run_enter(&mut app, &ctx); // sign with the phrase
        let TabState::Send { form, state } = &app.tab_state else {
            panic!()
        };
        assert!(matches!(form.stage, SendStage::Confirm { .. }), "{state:?}");
        assert_eq!(form.key.len(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn switching_wallet_throws_away_a_half_built_payment() {
        let (mut app, _wif, dir) = app_with_wallet("switch");
        if let TabState::Send { form, .. } = &mut app.tab_state {
            form.stage = SendStage::Key {
                psbt: "built-for-alice".into(),
            };
        }
        app.reset_send_flow();
        let TabState::Send { form, .. } = &app.tab_state else {
            panic!()
        };
        assert_eq!(form.stage, SendStage::Compose);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn node_tabs_report_an_unreachable_node() {
        for t in [Tab::NodeStatus, Tab::Fees] {
            let err = run_tab(&tab(t), &unreachable_ctx())
                .unwrap_err()
                .to_string();
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
        assert!(
            matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if xprv_input == "typed text")
        );
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
        assert!(matches!(
            &app.tab_state,
            TabState::Derive {
                state: OperationState::Idle,
                ..
            }
        ));
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
        if let TabState::KeysAndMnemonic {
            key_type_selected, ..
        } = &mut keys
        {
            *key_type_selected = true;
        }
        let out = run_job(&keys, &ctx).unwrap();
        assert!(out.text.contains("sent to the Derive tab"));
        let Some(Handoff::Xprv(xprv)) = out.handoff else {
            panic!("expected an xprv hand-off")
        };
        assert!(xprv.starts_with("tprv"), "{xprv}");

        let mut app = AppState::new();
        app.apply_handoff(Handoff::Xprv(xprv.clone()));
        app.next_tab();
        assert!(
            matches!(&app.tab_state, TabState::Derive { xprv_input, .. } if *xprv_input == xprv)
        );
        // ...and the Derive tab can actually use it.
        assert!(run_job(&app.tab_state, &ctx).is_ok());
    }

    #[test]
    fn a_generated_key_and_a_derived_key_pre_fill_the_addresses_tab() {
        let ctx = mock_ctx();
        let out = run_job(&tab(Tab::KeysAndMnemonic), &ctx).unwrap();
        let Some(Handoff::Pubkey(pubkey)) = out.handoff else {
            panic!("expected a pubkey hand-off")
        };
        let mut app = AppState::new();
        app.apply_handoff(Handoff::Pubkey(pubkey.clone()));
        app.next_tab();
        app.next_tab();
        assert!(
            matches!(&app.tab_state, TabState::Addresses { pubkey_input, .. } if *pubkey_input == pubkey)
        );
        assert!(
            run_job(&app.tab_state, &ctx).is_ok(),
            "the hand-off must be a valid public key"
        );
    }

    #[test]
    fn the_derive_tab_defaults_to_a_segwit_path_for_the_network() {
        let mut app = AppState::new();
        app.configure_network("MAINNET (real funds)".into(), true);
        app.next_tab();
        assert!(
            matches!(&app.tab_state, TabState::Derive { path_input, .. } if path_input == "m/84'/0'/0'/0/0")
        );
        let mut app = AppState::new();
        app.configure_network("regtest".into(), false);
        app.next_tab();
        assert!(
            matches!(&app.tab_state, TabState::Derive { path_input, .. } if path_input == "m/84'/1'/0'/0/0")
        );
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
        assert!(matches!(press(KeyCode::Esc), Action::Esc));
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
