use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum FeeCmd {
    Estimate {
        #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u16).range(1..=1008))]
        target: u16,

        #[arg(long, value_enum, default_value_t = FeeMode::Economical)]
        mode: FeeMode,

        #[arg(long, value_name = "SAT/VB")]
        fallback_rate: Option<f64>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FeeMode {
    Economical,
    Conservative,
}
