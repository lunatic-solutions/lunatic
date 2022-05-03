use std::{env, fs, path::Path, sync::Arc};

use anyhow::{Context, Result};
use clap::{crate_version, Arg, Command};

use dashmap::DashMap;
use lunatic_process::{runtimes, state::ProcessState};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{spawn_wasm, DefaultProcessConfig, DefaultProcessState};

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

    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

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
    // Forward `--bench` flag to process
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

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;

    // Spawn main process
    let module = fs::read(path)?;

    let module = runtime.compile_module::<DefaultProcessState>(module)?;

    let registry = Arc::new(DashMap::new());
    let state =
        DefaultProcessState::new(runtime.clone(), module.clone(), Arc::new(config), registry)
            .unwrap();
    let (task, _) = spawn_wasm(runtime, module, state, "_start", Vec::new(), None)
        .await
        .context(format!(
            "Failed to spawn process from {}::_start()",
            path.to_string_lossy()
        ))?;
    // Wait on the main process to finish
    task.await.map(|_| ())
}
