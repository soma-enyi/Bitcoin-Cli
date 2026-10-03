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

        /// Estimate from what blocks paid instead of asking the node: the last block of the
        /// window (default: the tip)
        #[arg(long, value_name = "HEIGHT", conflicts_with = "fallback_rate")]
        block: Option<u32>,

        /// How many blocks to read, ending at --block (default depends on --target:
        /// 3, 6, 24 or 144)
        #[arg(
            long,
            value_name = "N",
            value_parser = clap::value_parser!(u16).range(1..=1008),
            conflicts_with = "fallback_rate"
        )]
        blocks: Option<u16>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FeeMode {
    Economical,
    Conservative,
}
