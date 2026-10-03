pub mod app;
pub mod clipboard;
pub mod event;
pub mod send;
pub mod session;
pub mod ui;
pub mod wallet_modal;

use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;

use crate::app::Context;
use app::AppState;
use app::Tab;
use event::{Action, handle_input, run_job};
use std::sync::mpsc;
use std::thread;

pub fn run(ctx: &Context) -> Result<(), Box<dyn std::error::Error>> {
    // Read the wallet list before taking over the terminal, so a problem with it is
    // reported normally instead of behind the full-screen view.
    let wallets_path = crate::app::wallets::default_path(ctx.network);
    let wallets = match &wallets_path {
        Some(path) => Some(crate::app::wallets::Wallets::open(path, ctx.network)?),
        None => None,
    };
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let term_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(term_backend)?;

    let mut app = AppState::new();
    app.configure_network(
        if ctx.network.is_mainnet() {
            "MAINNET (real funds)".into()
        } else {
            ctx.network.to_string()
        },
        ctx.network.is_mainnet(),
    );
    app.network = ctx.network;
    app.wallets = wallets;
    app.session_path = wallets_path.map(|p| p.with_file_name("session.json"));
    if let Some(path) = &app.session_path {
        app.apply_session(session::SavedSession::load(path));
    }
    // Whatever goes wrong, give the terminal back (and so stop showing a recovery phrase).
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
        previous_hook(info);
    }));

    // Each result carries the generation of the Send form it was started for, so a late one
    // cannot land in a form the user has since changed or reset.
    let (tx, rx) = mpsc::channel::<(Tab, Option<u64>, Result<event::JobOutput, String>)>();
    let (mine_tx, mine_rx) = mpsc::channel::<Result<String, String>>();
    let mut in_flight = 0usize;

    // Scoped threads let jobs borrow `ctx`; the scope ends when the user quits.
    let result = thread::scope(|scope| {
        loop {
            terminal.draw(|frame| ui::draw(frame, &app))?;
            app.increment_frame();
            app.tick();

            while let Ok((tab, epoch, outcome)) = rx.try_recv() {
                in_flight = in_flight.saturating_sub(1);
                if app.is_stale(tab, epoch) {
                    continue;
                }
                match outcome {
                    Ok(out) => {
                        app.apply_outcome(tab, Ok(out.text));
                        if let Some(handoff) = out.handoff {
                            app.apply_handoff(handoff);
                        }
                    }
                    Err(err) => app.apply_outcome(tab, Err(err)),
                }
            }

            while let Ok(result) = mine_rx.try_recv() {
                in_flight = in_flight.saturating_sub(1);
                app.finish_mining(result);
            }
            if let Some(wallet) = app.mine_request.take() {
                let mine_tx = mine_tx.clone();
                in_flight += 1;
                scope.spawn(move || {
                    let result =
                        crate::app::node::mine_for_wallet(ctx, &wallet).map_err(|e| e.to_string());
                    let _ = mine_tx.send(result);
                });
            }

            if let Some(action) = event::poll_event() {
                let quitting = match &action {
                    Action::Quit => true,
                    Action::Esc => app.modal.is_none() && !app.esc_goes_back(),
                    Action::QuitKey => !app.is_text_tab(),
                    _ => false,
                };
                if quitting {
                    // Save, empty the clipboard and hand the terminal back first: waiting for a
                    // running job (a coin scan can take minutes) must not leave a recovery
                    // phrase on screen or the shell frozen.
                    app.finish();
                    let _ = disable_raw_mode();
                    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);
                    let _ = terminal.show_cursor();
                    if in_flight > 0 {
                        std::process::exit(0);
                    }
                    break Ok::<(), Box<dyn std::error::Error>>(());
                }
                match action {
                    Action::Esc if app.modal.is_none() => {
                        // Typing a key or confirming: Esc backs out instead of quitting.
                        handle_input(&mut app, Action::Tab);
                    }
                    Action::QuitKey => {
                        handle_input(&mut app, Action::CharInput('q'));
                    }
                    // The wallet dialog is modal: the tab underneath stays put.
                    Action::NextTab | Action::PrevTab if app.modal.is_some() => {}
                    Action::NextTab => app.next_tab(),
                    Action::PrevTab => app.prev_tab(),
                    _ => {
                        if let Some(job) = handle_input(&mut app, action) {
                            let tab = app.current_tab;
                            let epoch = match &job {
                                app::TabState::Send { form, .. } => Some(form.epoch),
                                _ => None,
                            };
                            let tx = tx.clone();
                            in_flight += 1;
                            scope.spawn(move || {
                                let outcome = run_job(&job, ctx).map_err(|e| e.to_string());
                                let _ = tx.send((tab, epoch, outcome));
                            });
                        }
                    }
                }
            }
        }
    });

    // Quitting already restored the terminal; this covers a draw error leaving the loop.
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}
