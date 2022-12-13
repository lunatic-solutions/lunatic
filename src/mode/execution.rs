use std::sync::Arc;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes::{self},
};

use super::common::{run_wasm, RunWasm};

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    wasm: super::common::WasmArgs,

    #[cfg(feature = "prometheus")]
    #[command(flatten)]
    prometheus: super::common::PrometheusArgs,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Control(super::control::Args),
    Node(super::node::Args),
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut args = Args::parse();

    if let Some(cmd) = args.command.take() {
        match cmd {
            Commands::Control(a) => super::control::start(a).await,
            Commands::Node(a) => super::node::start(a, args.wasm).await,
        }
    } else if args.wasm.path.is_some() {
        // Run wasm in non-distributed lunatic VM

        #[cfg(feature = "prometheus")]
        if args.prometheus.prometheus {
            super::common::prometheus(args.prometheus.prometheus_http, None)?;
        }

        // Create wasmtime runtime
        let wasmtime_config = runtimes::wasmtime::default_config();
        let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
        let envs = Arc::new(LunaticEnvironments::default());
        let env = envs.create(1).await;
        run_wasm(RunWasm {
            cli: args.wasm,
            runtime,
            envs,
            env,
            distributed: None,
        })
        .await
    } else {
        Err(anyhow!("Either provide a command or wasm"))
    }
}
