use std::{fs, path::Path};

use clap::{crate_version, App, Arg, ArgSettings};

use anyhow::{Context, Result};
use lunatic_runtime::{EnvConfig, Environment};

fn main() -> Result<()> {
    let args = App::new("lunatic")
        .version(crate_version!())
        .arg(
            Arg::new("plugin")
                .short('P')
                .long("plugin")
                .value_name("PLUGIN")
                .about("Adds plugin")
                .settings(&[ArgSettings::MultipleOccurrences, ArgSettings::TakesValue]),
        )
        .arg(
            Arg::new("wasm")
                .value_name("WASM")
                .about("Entry .wasm file")
                .required(true),
        )
        .get_matches();

    let mut config = EnvConfig::default();
    // Add plugins passed through the --plugin or -P flags to the environment
    if let Some(plugins) = args.values_of("plugin") {
        for plugin in plugins {
            let path = Path::new(plugin);
            let module = fs::read(path)?;
            config.add_plugin(module)?;
        }
    }
    let main_process_environment = Environment::new(config)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        // Spawn main process
        let path = args.value_of("wasm").unwrap();
        let path = Path::new(path);
        let module = fs::read(path)?;
        let module = main_process_environment.create_module(module).await?;
        main_process_environment
            .spawn(&module, "_start")
            .await
            .context(format!(
                "Failed to spawn process from {}::_start()",
                path.to_string_lossy()
            ))?
            .task
            .await?;
        Ok(())
    })
}
