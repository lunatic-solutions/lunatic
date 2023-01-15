use anyhow::Result;
use clap::{Parser, Subcommand};
use regex::Regex;
use std::collections::VecDeque;

#[derive(Parser, Debug)]
#[command(version)]
#[command(allow_external_subcommands(true))]
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
}

// Lunatic versions under 0.13 implied run
// This checks whether the 0.12 behaviour is wanted with a regex
fn is_run_implied() -> bool {
    if std::env::args().count() < 2 {
        return false;
    }

    // lunatic <foo.wasm> -> Implied run
    // lunatic run <foo.wasm> -> Explicit run
    // lunatic fdskl <foo.wasm> -> Not implied run
    let test_re = Regex::new(r"^(--bench|--dir|[^\s]+\.wasm)")
        .expect("BUG: Regex error with lunatic::mode::execution::is_run_implied()");

    test_re.is_match(&std::env::args().nth(1).unwrap())
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Run is implied from lunatic 0.12
    let args = if is_run_implied() {
        let mut augmented_args: VecDeque<String> = std::env::args().collect();
        let first_arg = augmented_args.pop_front().unwrap();
        augmented_args.push_front("run".to_owned());
        augmented_args.push_front(first_arg);
        Args::parse_from(augmented_args)
    } else {
        println!("Run is NOT implied!");
        Args::parse()
    };

    match args.command {
        Commands::Init => super::init::start(),
        Commands::Run(a) => super::run::start(a).await,
        Commands::Control(a) => super::control::start(a).await,
        Commands::Node(a) => super::node::start(a).await,
    }
}
