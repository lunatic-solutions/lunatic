use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes::{self},
};
use opentelemetry::{
    global::BoxedTracer,
    runtime::Tokio,
    trace::{noop::NoopTracer, Span, Tracer},
    Context, KeyValue,
};

use super::common::{run_wasm, RunWasm};

#[derive(Parser, Debug)]
#[command(version)]
#[command(group(
    clap::ArgGroup::new("tracer")
        .args(["jaeger"]),
))]
pub struct Args {
    /// Grant access to the given host directories
    #[arg(long, value_name = "DIRECTORY")]
    pub dir: Vec<PathBuf>,

    /// Indicate that a benchmark is running
    #[arg(long)]
    pub bench: bool,

    /// Jaeger connection url for tracing.
    #[arg(long)]
    pub jaeger: Option<String>,

    /// Entry .wasm file
    #[arg(index = 1)]
    pub path: PathBuf,

    /// Arguments passed to the guest
    #[arg(index = 2)]
    pub wasm_args: Vec<String>,

    #[cfg(feature = "prometheus")]
    #[command(flatten)]
    prometheus: super::common::PrometheusArgs,
}

pub(crate) async fn start(mut args: Args) -> Result<()> {
    #[cfg(feature = "prometheus")]
    if args.prometheus.prometheus {
        super::common::prometheus(args.prometheus.prometheus_http, None)?;
    }

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
    let envs = Arc::new(LunaticEnvironments::default());

    let env = envs.create(1).await;
    if args.bench {
        args.wasm_args.push("--bench".to_owned());
    }

    let tracer = match args.jaeger {
        Some(jaeger_url) => {
            opentelemetry::global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
            let tracer = opentelemetry_jaeger::new_agent_pipeline()
                .with_endpoint(jaeger_url)
                .with_service_name("lunatic")
                .install_batch(Tokio)?;
            Arc::new(BoxedTracer::new(Box::new(tracer)))
        }
        None => Arc::new(BoxedTracer::new(Box::new(NoopTracer::new()))),
    };

    run_wasm(RunWasm {
        path: args.path,
        wasm_args: args.wasm_args,
        dir: args.dir,
        runtime,
        envs,
        env,
        distributed: None,
        tracer,
    })
    .await
}
