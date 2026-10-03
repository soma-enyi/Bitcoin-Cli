mod app;
mod cli;
mod dispatch;
mod envsel;
mod output;
mod tuiapp;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

fn main() -> ExitCode {
    // Missing .env is fine; real environment variables still win over its values.
    if let Err(err) = dotenvy::dotenv() {
        if !err.not_found() {
            // The loader stops at the first line it cannot read, so every setting below it is
            // silently missing. Say so, but never print the line: it may be a secret.
            eprintln!(
                "warning: .env has a line that is not KEY=VALUE (or a comment starting with #), \
                 so the settings after it were NOT loaded. Fix or remove that line."
            );
        }
    }
    envsel::apply(&std::env::args().collect::<Vec<_>>());

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return if err.use_stderr() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
    };

    let mode = dispatch::output_mode(&cli);
    match dispatch::context(&cli).and_then(|ctx| dispatch::run(cli, &ctx)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            output::emit_error(&err, mode);
            ExitCode::from(err.exit_code())
        }
    }
}
