pub mod app;
pub mod event;
pub mod ui;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

use crate::app::Context;
use app::AppState;
use app::Tab;
use event::{Action, handle_input, run_job};
use std::sync::mpsc;
use std::thread;

pub fn run(ctx: &Context) -> Result<(), Box<dyn std::error::Error>> {
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
    let (tx, rx) = mpsc::channel::<(Tab, Result<event::JobOutput, String>)>();

    // Scoped threads let jobs borrow `ctx`; the scope ends when the user quits.
    let result = thread::scope(|scope| loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;
        app.increment_frame();

        while let Ok((tab, outcome)) = rx.try_recv() {
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

        if let Some(action) = event::poll_event() {
            match action {
                Action::Quit => break Ok::<(), Box<dyn std::error::Error>>(()),
                Action::QuitKey if !app.is_text_tab() => break Ok(()),
                Action::QuitKey => {
                    handle_input(&mut app, Action::CharInput('q'));
                }
                Action::NextTab => app.next_tab(),
                Action::PrevTab => app.prev_tab(),
                _ => {
                    if let Some(job) = handle_input(&mut app, action) {
                        let tab = app.current_tab;
                        let tx = tx.clone();
                        scope.spawn(move || {
                            let outcome = run_job(&job, ctx).map_err(|e| e.to_string());
                            let _ = tx.send((tab, outcome));
                        });
                    }
                }
            }
        }
    });

    disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}
