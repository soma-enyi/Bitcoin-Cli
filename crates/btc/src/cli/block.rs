use clap::Subcommand;

#[derive(Subcommand)]
pub enum BlockCmd {
    Info {
        #[arg(value_name = "HEIGHT|HASH")]
        block: String,
    },
}
