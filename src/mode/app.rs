use super::config::ConfigManager;
use anyhow::{anyhow, Result};
use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum AppArgs {
    /// Map a binary within the current repository to an `App` on the lunatic
    /// platform so that the correct binary is deployed when using `lunatic deploy`
    Add {
        /// The name of the App
        name: String,

        /// Bind App on remote to `bin` in repository
        #[arg(short = 'b', long)]
        bin: Option<String>,

        /// Bind App on remote to example in repository
        #[arg(short = 'e', long)]
        example: Option<String>,

        /// Bind App on remote to workspace member (package) in repository
        #[arg(short = 'e', long)]
        package: Option<String>,
    },
    Remove {
        /// The name of the App to remove
        name: String,
    },
    List,
}

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    app: AppArgs,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    match args.app {
        AppArgs::Add {
            name,
            bin,
            example,
            package,
        } => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.update_project_apps().await?;
            let app_to_update = match config.find_app(name.as_str()) {
                Some(matching_app) => matching_app,
                None => {
                    // create new app and store in list
                    config.create_new_app(name).await?
                }
            };
            app_to_update.example = example;
            app_to_update.bin = bin;
            app_to_update.package = package;

            config.flush()?;
        }
        AppArgs::Remove { name } => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.remove_app(name).await?;
        }
        AppArgs::List => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.update_project_apps().await?;

            println!("Available apps on remote:");
            for app in config.project_config.remote.into_iter() {
                let (mapping_type, mapping_path) = match (
                    app.bin.as_deref(),
                    app.example.as_deref(),
                    app.package.as_deref(),
                ) {
                    (None, None, None) => ("No mapping yet", ""),
                    (None, Some(example), None) => ("example ", example),
                    (Some(bin), None, None) => ("bin ", bin),
                    (None, None, Some(package)) => ("package/workspace member ", package),
                    _ => ("WARNING! Multiple mappings found for app", ""),
                };
                println!(
                    "- Name: \"{}\". Id: {}. Mapping: {}{}",
                    app.app_name, app.app_id, mapping_type, mapping_path
                );
            }
        }
    }
    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}
