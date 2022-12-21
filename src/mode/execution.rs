use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,

    #[cfg(feature = "prometheus")]
    #[command(flatten)]
    prometheus: super::common::PrometheusArgs,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(super::run::Args),
    Control(super::control::Args),
    Node(super::node::Args),
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    match args.command {
        Commands::Run(a) => super::run::start(a).await,
        Commands::Control(a) => super::control::start(a).await,
        Commands::Node(a) => super::node::start(a).await,
    }
}
