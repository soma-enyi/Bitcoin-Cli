use clap::Subcommand;

#[derive(Subcommand)]
pub enum NodeCmd {
    Status,
}
