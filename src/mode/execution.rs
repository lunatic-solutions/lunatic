use std::{env, fs, path::Path, sync::Arc};

use anyhow::{anyhow, Context, Ok, Result};
use clap::{crate_version, Arg, Command};
use tokio::sync::mpsc::channel;

use lunatic_distributed::{
    control::{self, server::control_server},
    distributed::{self, server::ServerCtx},
    quic,
};
use lunatic_process::{
    env::Environments,
    runtimes::{self, Modules, RawWasm},
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};

use uuid::Uuid;

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
            Arg::new("test_ca")
                .long("test-ca")
                .help("Use test Certificate Authority for bootstrapping QUIC connections.")
                .requires("control")
        )
        .arg(
            Arg::new("ca_cert")
                .long("ca-cert")
                .help("Certificate Authority public certificate used for boostraping QUIC connections.")
                .requires("control")
                .conflicts_with("test_ca")
                .takes_value(true)
        )
        .arg(
            Arg::new("ca_key")
                .long("ca-key")
                .help("Certificate Authority private key used for signing node certificate requests")
                .requires("control_server")
                .conflicts_with("test_ca")
                .takes_value(true)
        )
        .arg(
            Arg::new("no_entry")
                .long("no-entry")
                .help("If provided will join other nodes, but not require a .wasm entry file")
                .required_unless_present("wasm")
        ).arg(
            Arg::new("bench")
                .long("bench")
                .help("Indicate that a benchmark is running"),
        )
        .arg(
            Arg::new("wasm")
                .value_name("WASM")
                .help("Entry .wasm file")
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

    if args.is_present("test_ca") {
        log::warn!("Do not use test Certificate Authority in production!")
    }

    // Run control server
    if args.is_present("control_server") {
        if let Some(control_address) = args.value_of("control") {
            // TODO unwrap, better message
            let ca_cert = lunatic_distributed::control::server::root_cert(
                args.is_present("test_ca"),
                args.value_of("ca_cert"),
                args.value_of("ca_key"),
            )
            .unwrap();
            tokio::task::spawn(control_server(control_address.parse().unwrap(), ca_cert));
        }
    }

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
    let mut envs = Environments::default();

    let env = envs.get_or_create(1);

    let (distributed_state, control_client, node_id) =
        if let (Some(node_address), Some(control_address)) =
            (args.value_of("node"), args.value_of("control"))
        {
            // TODO unwrap, better message
            let node_address = node_address.parse().unwrap();
            let node_name = Uuid::new_v4().to_string();
            let control_address = control_address.parse().unwrap();
            let ca_cert = lunatic_distributed::distributed::server::root_cert(
                args.is_present("test_ca"),
                args.value_of("ca_cert"),
            )
            .unwrap();
            let node_cert =
                lunatic_distributed::distributed::server::gen_node_cert(&node_name).unwrap();

            let quic_client = quic::new_quic_client(&ca_cert).unwrap();

            let (node_id, control_client, signed_cert_pem) = control::Client::register(
                node_address,
                node_name.to_string(),
                control_address,
                quic_client.clone(),
                node_cert.serialize_request_pem().unwrap(),
            )
            .await?;

            let distributed_client =
                distributed::Client::new(node_id, control_client.clone(), quic_client.clone())
                    .await?;

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
                signed_cert_pem,
                node_cert.serialize_private_key_pem(),
            ));

            log::info!("Registration successful, node id {}", node_id);

            (Some(dist), Some(control_client), Some(node_id))
        } else {
            (None, None, None)
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

        let (task, _) = env
            .spawn_wasm(runtime, &module, state, "_start", Vec::new(), None)
            .await
            .context(format!(
                "Failed to spawn process from {}::_start()",
                path.to_string_lossy()
            ))?;
        // Wait on the main process to finish
        let result = task.await.map(|_| ()).map_err(|e| anyhow!(e.to_string()));

        // Until we refactor registration and reconnect authentication, send node id explicitly
        if let (Some(ctrl), Some(node_id)) = (control_client, node_id) {
            ctrl.deregister(node_id).await;
        }

        result
    } else {
        // Block forever
        let (_sender, mut receiver) = channel::<()>(1);
        receiver.recv().await.unwrap();
        Ok(())
    }
}
