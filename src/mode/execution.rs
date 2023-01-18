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
    /// Create a new project
    New(super::new::Args),
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    match args.command {
        Commands::Init => super::init::start(),
        Commands::Run(a) => super::run::start(a).await,
        Commands::Control(a) => super::control::start(a).await,
        Commands::Node(a) => super::node::start(a).await,
        Commands::New(a) => super::new::start(a),
    }
}
