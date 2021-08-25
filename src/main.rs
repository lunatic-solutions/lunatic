use std::{env, fs, path::Path};

use clap::{crate_version, App, Arg, ArgSettings};

use anyhow::{Context, Result};
use lunatic_runtime::{EnvConfig, Environment};

fn main() -> Result<()> {
    // Init logger
    env_logger::init();
    // Parse command line arguments
    let args = App::new("lunatic")
        .version(crate_version!())
        .arg(
            Arg::new("plugin")
                .short('P')
                .long("plugin")
                .value_name("PLUGIN")
                .about("Adds plugin")
                .setting(ArgSettings::MultipleOccurrences)
                .setting(ArgSettings::TakesValue),
        )
        .arg(
            Arg::new("wasm")
                .value_name("WASM")
                .about("Entry .wasm file")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("wasm_args")
                .value_name("WASM_ARGS")
                .about("Arguments passed to the guest")
                .required(false)
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

    // Add plugins passed through the --plugin or -P flags to the environment
    if let Some(plugins) = args.values_of("plugin") {
        for plugin in plugins {
            let path = Path::new(plugin);
            let module = fs::read(path)?;
            config.add_plugin(module)?;
        }
    }
    let env = Environment::new(config)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
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
        task.await?;
        Ok(())
    })
}
