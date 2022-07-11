use std::{env, fs, path::Path, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use clap::{crate_version, Arg, Command};
use tokio::sync::mpsc::channel;

use lunatic_distributed::{
    connection::new_quic_client,
    control::{self, server::control_server},
    distributed::{self, server::ServerCtx},
};
use lunatic_process::{
    env::Environments,
    runtimes::{self, Modules, RawWasm},
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};

pub(crate) async fn execute() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Parse command line arguments
    let args = Command::new("lunatic")
        .version(crate_version!())
        .arg(
            Arg::new("dir")
                .long("dir")
                .value_name("DIRECTORY")
                .help("Grant access to the given host directory")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("node")
                .long("node")
                .value_name("NODE_ADDRESS")
                .help("Turns local process into a node and binds it to the provided address.")
                .requires("control")
                .takes_value(true),
        )
        .arg(
            Arg::new("node_name")
            .long("node-name")
            .value_name("NODE_NAME")
            .help("Name of the node under which it registers to the control node.")
            .requires("control")
            .takes_value(true)
        )
        .arg(
            Arg::new("control")
                .long("control")
                .value_name("CONTROL_ADDRESS")
                .help("Address of a control node inside the cluster that will be used for bootstrapping.")
                .takes_value(true)
        )
        .arg(
            Arg::new("control_name")
            .long("control-name")
            .value_name("CONTROL_NAME")
            .help("Name of a control node inside the cluster that will be used for bootstrapping.")
            .takes_value(true)
        )
        .arg(
            Arg::new("ca_cert")
                .long("ca_cert")
                .value_name("CA_CERT")
                .help("Publicly trusted digital certificate used by QUIC client.")
                .takes_value(true)
                .requires("control")
        )
        .arg(
            Arg::new("cert")
                .long("cert")
                .value_name("CERT")
                .help("Signed digital certificate used by QUIC server.")
                .takes_value(true)
                .requires("control")
        )
        .arg(
            Arg::new("key")
                .long("key")
                .value_name("KEY")
                .help("Private key used by QUIC server.")
                .takes_value(true)
                .requires("control")
        )
        .arg(
            Arg::new("control_server")
                .long("control-server")
                .help("When set run the control server")
                .requires("control"),
        )
        .arg(
            Arg::new("no_entry")
                .long("no-entry")
                .help("If provided will join other nodes, but not require a .wasm entry file")
                .requires("node"),
        ).arg(
            Arg::new("bench")
                .long("bench")
                .help("Indicate that a benchmark is running"),
        )
        .arg(
            Arg::new("wasm")
                .value_name("WASM")
                .help("Entry .wasm file")
                .required_unless_present("no_entry")
                .conflicts_with("no_entry")
                .index(1),
        )
        .arg(
            Arg::new("wasm_args")
                .value_name("WASM_ARGS")
                .help("Arguments passed to the guest")
                .required(false)
                .conflicts_with("no_entry")
                .multiple_values(true)
                .index(2),
        )
        .get_matches();

    // Run control server
    if args.is_present("control_server") {
        if let Some(control_address) = args.value_of("control") {
            // TODO unwrap, better message
            let cert = args.value_of("cert").unwrap().to_string();
            let key = args.value_of("key").unwrap().to_string();
            tokio::task::spawn(control_server(control_address.parse().unwrap(), cert, key));
        }
    }

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
    let mut envs = Environments::default();

    let env = envs.get_or_create(1);

    let distributed_state = if let (
        Some(node_address),
        Some(node_name),
        Some(control_address),
        Some(control_name),
        Some(ca_cert),
        Some(cert),
        Some(key),
    ) = (
        args.value_of("node"),
        args.value_of("node_name"),
        args.value_of("control"),
        args.value_of("control_name"),
        args.value_of("ca_cert"),
        args.value_of("cert"),
        args.value_of("key"),
    ) {
        // TODO unwrap, better message
        let node_address = node_address.parse().unwrap();
        let control_address = control_address.parse().unwrap();
        let control_name = control_name.to_string();
        let ca_cert = ca_cert.to_owned();
        let quic_client = new_quic_client(node_address, ca_cert).unwrap();
        let (node_id, control_client) = control::Client::register(
            node_address,
            node_name.to_string(),
            control_address,
            control_name,
            quic_client.clone(),
        )
        .await?;
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
            cert.to_string(),
            key.to_string(),
        ));

        log::info!("Registration successful, node id {}", node_id);

        Some(dist)
    } else {
        None
    };

    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

    if !args.is_present("no_entry") {
        // Path to wasm file
        let path = args.value_of("wasm").unwrap();
        let path = Path::new(path);

        // Set correct command line arguments for the guest
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let mut wasi_args = vec![filename];
        let wasm_args = args
            .values_of("wasm_args")
            .unwrap_or_default()
            .map(|arg| arg.to_string());
        wasi_args.extend(wasm_args);
        if args.is_present("bench") {
            wasi_args.push("--bench".to_owned());
        }
        config.set_command_line_arguments(wasi_args);

        // Inherit environment variables
        config.set_environment_variables(env::vars().collect());

        // Always preopen the current dir
        config.preopen_dir(".");
        if let Some(dirs) = args.values_of("dir") {
            for dir in dirs {
                config.preopen_dir(dir);
            }
        }

        // Spawn main process
        let module = fs::read(path)?;
        let module: RawWasm = if let Some(dist) = distributed_state.as_ref() {
            dist.control.add_module(module).await?
        } else {
            module.into()
        };
        let module = runtime.compile_module::<DefaultProcessState>(module)?;
        let state = DefaultProcessState::new(
            env.clone(),
            distributed_state,
            runtime.clone(),
            module.clone(),
            Arc::new(config),
            Default::default(),
        )
        .unwrap();

        let (task, _) = env
            .spawn_wasm(runtime, module, state, "_start", Vec::new(), None)
            .await
            .context(format!(
                "Failed to spawn process from {}::_start()",
                path.to_string_lossy()
            ))?;
        // Wait on the main process to finish
        task.await.map(|_| ()).map_err(|e| anyhow!(e.to_string()))
    } else {
        // Block forever
        let (_sender, mut receiver) = channel::<()>(1);
        receiver.recv().await.unwrap();
        Ok(())
    }
}
