use std::{env, fs, path::Path, sync::Arc};

use anyhow::{Context, Result};
use async_std::channel;
use clap::{crate_version, Arg, Command};

use lunatic_control::server::control_server;
use lunatic_distributed::node::Node;
use lunatic_process::{
    env::Environment, local_control::local_control, runtimes, state::ProcessState,
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};

pub(crate) async fn execute() -> Result<()> {
    // Init logger
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
            Arg::new("control")
                .long("control")
                .value_name("CONTROL_ADDRESS")
                .help("Address of a control node inside the cluster that will be used for bootstrapping.")
                .takes_value(true)
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
            async_std::task::spawn(control_server(control_address.parse().unwrap()));
        }
    }

    let control = if let (Some(node_address), Some(control_address)) =
        (args.value_of("node"), args.value_of("control"))
    {
        // TODO unwrap, better message
        let ctrl = lunatic_control::client::register(
            node_address.parse().unwrap(),
            control_address.parse().unwrap(),
        )
        .await?;
        log::info!("Registration successful, node id {}", ctrl.node_id);
        let node = Node::new();
        async_std::task::spawn(lunatic_distributed::server::node_server(
            node,
            node_address.parse().unwrap(),
        ));
        ctrl
    } else {
        local_control()
    };

    let distributed = lunatic_distributed::client::start_client(control.clone()).await?;

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

        // Create wasmtime runtime
        let wasmtime_config = runtimes::wasmtime::default_config();
        let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;

        // Spawn main process
        let module = fs::read(path)?;

        let module = runtime.compile_module::<DefaultProcessState>(module)?;

        let env = Environment::new(1, control, distributed);
        let state = DefaultProcessState::new(
            env.clone(),
            runtime.clone(),
            module.clone(),
            Arc::new(config),
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
        task.await.unwrap();
    } else {
        // Block forever
        let (_sender, receiver) = channel::bounded(1);
        let _: () = receiver.recv().await.unwrap();
    }

    Ok(())
}
