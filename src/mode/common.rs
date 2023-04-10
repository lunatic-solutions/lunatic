use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context, Result};
use clap::Args;

use hyper::{
    header::CONTENT_TYPE,
    http,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use lunatic_distributed::DistributedProcessState;
use lunatic_process::{
    env::{Environment, LunaticEnvironment, LunaticEnvironments},
    runtimes::{wasmtime::WasmtimeRuntime, RawWasm},
    wasm::spawn_wasm,
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};
use opentelemetry::{
    global::{self, BoxedTracer, GlobalMeterProvider},
    metrics::MetricsError,
    sdk::metrics::MeterProvider,
    trace::{Span, TraceContextExt, Tracer},
    KeyValue,
};
use prometheus::{Encoder, Registry, TextEncoder};

#[derive(Args, Debug)]
pub struct WasmArgs {}

pub struct RunWasm {
    pub path: PathBuf,
    pub wasm_args: Vec<String>,
    pub dir: Vec<PathBuf>,

    pub runtime: WasmtimeRuntime,
    pub envs: Arc<LunaticEnvironments>,
    pub env: Arc<LunaticEnvironment>,
    pub distributed: Option<DistributedProcessState>,
    pub tracer: Arc<BoxedTracer>,
    pub meter_provider: GlobalMeterProvider,
}

pub async fn run_wasm(args: RunWasm) -> Result<()> {
    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

    // Path to wasm file
    let path = args.path;

    // Set correct command line arguments for the guest
    let filename = path.file_name().unwrap().to_string_lossy().to_string();
    let mut wasi_args = vec![filename];
    wasi_args.extend(args.wasm_args);
    config.set_command_line_arguments(wasi_args);

    // Inherit environment variables
    config.set_environment_variables(std::env::vars().collect());

    // Always preopen the current dir
    config.preopen_dir(".");
    for dir in args.dir {
        if let Some(s) = dir.as_os_str().to_str() {
            config.preopen_dir(s);
        }
    }

    // Spawn main process
    let module = std::fs::read(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => anyhow!("Module '{}' not found", path.display()),
        _ => err.into(),
    })?;
    let module: RawWasm = if let Some(dist) = args.distributed.as_ref() {
        dist.control.add_module(module).await?
    } else {
        module.into()
    };

    let module = Arc::new(args.runtime.compile_module::<DefaultProcessState>(module)?);
    let mut root_span = args.tracer.start(
        "app_start", // SpanBuilder::from_name("app_start")
                     //     .with_attributes([KeyValue::new("path", path.to_string_lossy().to_string())]),
    );
    root_span.set_attributes([
        KeyValue::new("service.name", "lunatic"),
        KeyValue::new("lunatic.path", path.to_string_lossy().to_string()),
        KeyValue::new("lunatic.environment", args.env.id() as i64),
    ]);
    let tracer_context = Arc::new(opentelemetry::Context::new().with_span(root_span));
    let logger = Arc::new(
        env_logger::Builder::from_env(
            env_logger::Env::new()
                .filter_or("LUNATIC_LOG", "info")
                .write_style("LUNATIC_LOG_STYLE"),
        )
        .build(),
    );
    let state = DefaultProcessState::new(
        args.env.clone(),
        args.distributed,
        args.runtime.clone(),
        module.clone(),
        Arc::new(config),
        Default::default(),
        args.tracer,
        tracer_context,
        args.meter_provider,
        logger,
    )
    .unwrap();

    args.env.can_spawn_next_process().await?;

    let (task, _) = spawn_wasm(
        args.env.clone(),
        args.runtime,
        &module,
        state,
        "_start",
        Vec::new(),
        None,
    )
    .await
    .context(format!(
        "Failed to spawn process from {}::_start()",
        path.to_string_lossy()
    ))?;

    // Wait on the main process to finish, or ctrl c signal
    tokio::select! {
        result = task => {
            result.map(|_| ()).map_err(|err| anyhow!(err.to_string()))
        },
        Ok(_) = tokio::signal::ctrl_c() => {
            args.env.shutdown();
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            Ok(())
        },
    }
}

pub fn prometheus(addr: &SocketAddr, _node_id: Option<u64>) -> Result<MeterProvider, MetricsError> {
    let registry = Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()?;
    let provider = MeterProvider::builder().with_reader(exporter).build();
    global::set_meter_provider(provider.clone());

    async fn handle_request(
        req: Request<Body>,
        registry: Registry,
    ) -> Result<Response<Body>, http::Error> {
        match (req.method(), req.uri().path()) {
            (&Method::GET, "/metrics") => {
                let mut buffer = vec![];
                let encoder = TextEncoder::new();
                let metric_families = registry.gather();
                if let Err(err) = encoder.encode(&metric_families, &mut buffer) {
                    log::error!("failed to encode prometheus metrics: {err}");
                    return Response::builder().status(400).body(Body::empty());
                }

                Response::builder()
                    .status(200)
                    .header(CONTENT_TYPE, encoder.format_type())
                    .body(Body::from(buffer))
            }
            _ => Response::builder()
                .status(404)
                .body(Body::from("not found")),
        }
    }

    let server =
        Server::bind(addr).serve(make_service_fn(move |_conn| {
            let registry = registry.clone();

            async move {
                Ok::<_, http::Error>(service_fn(move |req| handle_request(req, registry.clone())))
            }
        }));

    if addr.ip().is_loopback() || addr.ip().is_unspecified() {
        log::info!(
            "prometheus metrics available at http://localhost:{}/metrics",
            addr.port()
        );
    } else {
        log::info!(
            "prometheus metrics available at http://{}:{}/metrics",
            addr.ip(),
            addr.port()
        );
    }

    tokio::spawn(server);
    // metrics_exporter_prometheus::PrometheusBuilder::new()
    //     .with_http_listener(http_socket.unwrap_or_else(|| "0.0.0.0:9927".parse().unwrap()))
    //     .add_global_label("node_id", node_id.unwrap_or(0).to_string())
    //     .install()?;
    Ok(provider)
}
