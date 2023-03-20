use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;

use super::config::{ConfigManager, ProjectLunaticConfig};

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum RemoteArgs {
    Add {
        /// The id of the remote
        id: String,
        /// The URL of the remote
        url: String,
    },
    // Rename {
    //     /// The old name of the remote
    //     old_name: String,
    //     /// The new name of the remote
    //     new_name: String,
    // },
    Remove,
    Get {
        /// The name of the remote
        #[arg(long)]
        name: Option<String>,
        /// The url of the remote
        #[arg(long)]
        url: Option<String>,
        /// The id of the remote
        #[arg(long)]
        id: Option<String>,
    },
}

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    remote: RemoteArgs,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    match args.remote {
        RemoteArgs::Add { id, url } => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.project_config.project_url = url;
            config.project_config.project_id = id;
            config.lookup_project().await?;
        }
        // RemoteArgs::Rename { old_name, new_name } => todo!(),
        RemoteArgs::Remove => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.project_config = ProjectLunaticConfig::default();
            config.flush()?;
        }
        RemoteArgs::Get { name, url, id } => {
            let config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            if name.is_some() {
                info!("{}", config.project_config.project_name);
            } else if url.is_some() {
                info!("{}", config.project_config.project_url);
            } else if id.is_some() {
                info!("{}", config.project_config.project_id);
            }
        }
    }
    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}
