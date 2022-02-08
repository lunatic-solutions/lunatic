use std::{env, fs, path::Path};

use async_std::channel;
use clap::{crate_version, App, Arg, ArgSettings};

use anyhow::{Context, Result};
use lunatic_runtime::{node::Node, EnvConfig, Environment, NODE};

#[async_std::main]
async fn main() -> Result<()> {
    // Init logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Parse command line arguments
    let args = App::new("lunatic")
        .version(crate_version!())
        .arg(
            Arg::new("dir")
                .long("dir")
                .value_name("DIRECTORY")
                .help("Grant access to the given host directory")
                .setting(ArgSettings::MultipleOccurrences)
                .setting(ArgSettings::TakesValue),
        )
        .arg(
            Arg::new("node")
                .long("node")
                .value_name("NODE_ADDRESS")
                .help("Turns local process into a node and binds it to the provided address.")
                .requires("node_name")
                .setting(ArgSettings::TakesValue),
        )
        .arg(
            Arg::new("node_name")
                .long("node-name")
                .value_name("NODE_NAME")
                .help("Name of the node.")
                .requires("node")
                .setting(ArgSettings::TakesValue),
        )
        .arg(
            Arg::new("peer")
                .long("peer")
                .value_name("PEER_ADDRESS")
                .help("Address of another node inside the cluster that will be used for bootstrapping.")
                .setting(ArgSettings::TakesValue)
                .requires("node"),
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

    let mut config = EnvConfig::default();

    // Set correct command line arguments for the guest
    let wasi_args = args
        .values_of("wasm_args")
        .unwrap_or_default()
        .map(|arg| arg.to_string())
        .collect();
    config.set_wasi_args(wasi_args);

    // Inherit environment variables
    config.set_wasi_envs(env::vars().collect());

    if let Some(dirs) = args.values_of("dir") {
        for dir in dirs {
            config.preopen_dir(dir);
        }
    }
    let env = Environment::local(config)?;

    // Setup a node if flag is set
    if let Some(node_address) = args.value_of("node") {
        let name = args.value_of("node_name").unwrap().to_string();
        let peer = args.value_of("peer");
        let node = Node::new(name, node_address, peer).await?;

        *NODE.write().await = Some(node);
    }

    if args.is_present("no_entry") {
        // Block forever
        let (_sender, receiver) = channel::bounded(1);
        let _: () = receiver.recv().await.unwrap();
    } else {
        // Spawn main process
        let path = args.value_of("wasm").unwrap();
        let path = Path::new(path);
        let module = fs::read(path)?;
        let module = env.create_module(module).await?;
        let (task, _) = module
            .spawn("_start", Vec::new(), None)
            .await
            .context(format!(
                "Failed to spawn process from {}::_start()",
                path.to_string_lossy()
            ))?;
        // Wait on the main process to finish
        task.await;
    }
    Ok(())
}
