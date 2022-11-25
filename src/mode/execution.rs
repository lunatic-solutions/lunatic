use std::{collections::HashMap, env, fs, path::Path, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use clap::Parser;
use lunatic_distributed::{
    control::{self},
    distributed::{self, server::ServerCtx},
    quic,
};
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes::{self, Modules, RawWasm},
    wasm::spawn_wasm,
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};
use reqwest::Url;
use tokio::sync::mpsc::channel;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Grant access to the given host directories
    #[arg(long, value_name = "DIRECTORY")]
    dir: Vec<String>,

    /// Turns local process into a node and binds it to the provided address
    #[arg(long, value_name = "NODE_ADDRESS", requires = "control")]
    node: Option<String>,

    /// URL of a control server
    #[arg(long, value_name = "CONTROL_URL")]
    control: Option<Url>,

    /// Define key=value variable to store as node information
    /// TODO: parse with URL query string parser?
    //#[arg(long, value_parser = parse_key_val, action = clap::ArgAction::Append)]
    //tag: Vec<(String, String)>,

    /// If provided will join other nodes, but not require a .wasm entry file
    #[arg(long, required_unless_present = "wasm")]
    no_entry: bool,

    /// Indicate that a benchmark is running
    #[arg(long)]
    bench: bool,

    /// Entry .wasm file
    #[arg(conflicts_with = "no_entry", index = 1)]
    wasm: Option<String>,

    /// Arguments passed to the guest
    #[arg(conflicts_with = "no_entry", index = 2)]
    wasm_args: Vec<String>,

    /// Enables the prometheus metrics exporter
    #[cfg(feature = "prometheus")]
    #[arg(long)]
    prometheus: bool,

    /// Address to bind the prometheus http listener to
    #[cfg(feature = "prometheus")]
    #[arg(
        long,
        value_name = "PROMETHEUS_HTTP_ADDRESS",
        requires = "prometheus",
        default_value_t = "0.0.0.0:9927"
    )]
    prometheus_http: String,
}

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args = Args::parse();

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
    let envs = Arc::new(LunaticEnvironments::default());

    let env = envs.create(1).await;
    let http_client = reqwest::Client::new();

    let (distributed_state, control_client) = if let (Some(node_address), Some(control_url)) =
        (args.node, args.control)
    {
        // TODO unwrap, better message
        let node_address = node_address.parse().unwrap();
        let node_name = Uuid::new_v4();
        let node_name_str = node_name.as_hyphenated().to_string();
        let node_attributes: HashMap<String, String> = Default::default(); //args.tag.into_iter().collect(); TODO
        let node_cert =
            lunatic_distributed::distributed::server::gen_node_cert(&node_name_str).unwrap();
        log::info!("Generate CSR for node name {node_name_str}");

        let control_client = control::Client::register(
            node_address,
            node_name,
            node_attributes,
            control_url,
            http_client.clone(),
            node_cert.serialize_request_pem()?,
        )
        .await?;

        let reg = control_client.reg();
        let node_id = reg.node_id as u64;

        log::info!("Registration successful, node id {}", node_id);

        let quic_client = quic::new_quic_client(&reg.root_cert).unwrap();

        let distributed_client =
            distributed::Client::new(node_id, control_client.clone(), quic_client.clone()).await?;

        let dist = lunatic_distributed::DistributedProcessState::new(
            node_id,
            control_client.clone(),
            distributed_client,
        )
        .await?;

        tokio::task::spawn(lunatic_distributed::distributed::server::node_server(
            ServerCtx {
                envs,
                modules: Modules::<DefaultProcessState>::default(),
                distributed: dist.clone(),
                runtime: runtime.clone(),
            },
            node_address,
            reg.cert_pem,
            node_cert.serialize_private_key_pem(),
        ));

        (Some(dist), Some(control_client))
    } else {
        (None, None)
    };

    #[cfg(feature = "prometheus")]
    if args.is_present("prometheus") {
        let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
        let builder = if let Some(addr) = args.value_of("prometheus_http") {
            builder.with_http_listener(addr.parse::<std::net::SocketAddr>().unwrap())
        } else {
            builder
        };

        let builder = if let Some(node_id) = node_id {
            builder.add_global_label("node_id", node_id.to_string())
        } else {
            builder
        };

        builder.install().unwrap()
    }

    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

    if args.no_entry {
        // Block forever
        let (_sender, mut receiver) = channel::<()>(1);
        receiver.recv().await.unwrap();
        return Ok(());
    }

    // Path to wasm file
    let path = args.wasm.unwrap();
    let path = Path::new(&path);

    // Set correct command line arguments for the guest
    let filename = path.file_name().unwrap().to_string_lossy().to_string();
    let mut wasi_args = vec![filename];
    wasi_args.extend(args.wasm_args);
    if args.bench {
        wasi_args.push("--bench".to_owned());
    }
    config.set_command_line_arguments(wasi_args);

    // Inherit environment variables
    config.set_environment_variables(env::vars().collect());

    // Always preopen the current dir
    config.preopen_dir(".");
    for dir in args.dir {
        config.preopen_dir(dir);
    }

    // Spawn main process
    let module = fs::read(path)?;
    let module: RawWasm = if let Some(dist) = distributed_state.as_ref() {
        dist.control.add_module(module).await?
    } else {
        module.into()
    };
    let module = Arc::new(runtime.compile_module::<DefaultProcessState>(module)?);
    let state = DefaultProcessState::new(
        env.clone(),
        distributed_state,
        runtime.clone(),
        module.clone(),
        Arc::new(config),
        Default::default(),
    )
    .unwrap();

    let (task, _) = spawn_wasm(env, runtime, &module, state, "_start", Vec::new(), None)
        .await
        .context(format!(
            "Failed to spawn process from {}::_start()",
            path.to_string_lossy()
        ))?;
    // Wait on the main process to finish
    let result = task.await.map(|_| ()).map_err(|e| anyhow!(e.to_string()));

    // Until we refactor registration and reconnect authentication, send node id explicitly
    if let Some(ctrl) = control_client {
        ctrl.notify_node_stopped().await.ok();
    }

    result
}
