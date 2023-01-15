use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version)]
#[command(allow_external_subcommands(true))]
pub struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[cfg(feature = "prometheus")]
    #[command(flatten)]
    prometheus: super::common::PrometheusArgs,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize a Rust cargo project as a lunatic project
    ///
    /// This command should be run inside the root folder of a cargo project,
    /// containing the Cargo.toml file. It will add configuration options to it
    /// in the `cargo/config.toml` file, setting the compilation target to
    /// `wasm32-wasi` and the default runner for this target to `lunatic run`.
    Init,
    /// Executes a .wasm file
    Run(super::run::Args),
    /// Starts a control node
    Control(super::control::Args),
    /// Starts a node
    Node(super::node::Args),
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    match args.command {
        Some(Commands::Init) => super::init::start(),
        Some(Commands::Run(a)) => super::run::start(a).await,
        Some(Commands::Control(a)) => super::control::start(a).await,
        Some(Commands::Node(a)) => super::node::start(a).await,
        // Run with sole .wasm argument
        None => super::run::start(super::run::Args::parse()).await,
    }
}
