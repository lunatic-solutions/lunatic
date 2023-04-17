use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes,
};
use opentelemetry::{
    global::{BoxedTracer, GlobalMeterProvider},
    metrics::noop::NoopMeterProvider,
    runtime::Tokio,
    trace::noop::NoopTracer,
};

use super::common::{prometheus, run_wasm, RunWasm};

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    /// Grant access to the given host directories
    #[arg(long, value_name = "DIRECTORY")]
    pub dir: Vec<PathBuf>,

    /// Indicate that a benchmark is running
    #[arg(long)]
    pub bench: bool,

    /// Jaeger connection url for tracing.
    #[arg(
        long,
        value_name = "JAEGER_HTTP_ADDRESS",
        num_args(0..=1),
        require_equals(true),
        default_missing_value("127.0.0.1:6831")
    )]
    pub jaeger: Option<SocketAddr>,

    /// Address to bind the prometheus http listener to
    #[arg(
        long,
        value_name = "PROMETHEUS_HTTP_ADDRESS",
        num_args(0..=1),
        require_equals(true),
        default_missing_value("0.0.0.0:9927")
    )]
    pub prometheus: Option<SocketAddr>,

    /// Entry .wasm file
    #[arg(index = 1)]
    pub path: PathBuf,

    /// Arguments passed to the guest
    #[arg(index = 2)]
    pub wasm_args: Vec<String>,
}

pub(crate) async fn start(mut args: Args) -> Result<()> {
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
                .with_auto_split_batch(true)
                .install_batch(Tokio)?;
            Arc::new(BoxedTracer::new(Box::new(tracer)))
        }
        None => Arc::new(BoxedTracer::new(Box::new(NoopTracer::new()))),
    };

    let meter_provider = match &args.prometheus {
        Some(url) => GlobalMeterProvider::new(
            prometheus(url, None).expect("failed to create prometheus registry"),
        ),
        None => GlobalMeterProvider::new(NoopMeterProvider::new()),
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
        meter_provider,
    })
    .await
}
