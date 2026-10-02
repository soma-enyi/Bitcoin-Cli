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
    let _ = dotenvy::dotenv();
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
