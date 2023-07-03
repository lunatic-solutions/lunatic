use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;

use super::config::{ConfigManager, ProjectLunaticConfig};

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum ProjectArgs {
    Set {
        /// The id of the platform project
        id: String,
        // / The name of the platform project
        // name: String,
    },
    Remove,
    Get {
        /// The name of the platform project
        #[arg(long, short)]
        name: bool,
        /// The url of the platform project
        #[arg(long)]
        url: bool,
        /// The id of the platform project
        #[arg(long)]
        id: bool,
    },
}

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    project: ProjectArgs,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    match args.project {
        ProjectArgs::Set { id } => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.project_config_mut()?.project_url = format!("/api/projects/{id}");
            config.project_config_mut()?.project_id = id;
            config.lookup_project().await?;
            config.flush()?;
        }
        // ProjectArgs::Rename { old_name, new_name } => todo!(),
        ProjectArgs::Remove => {
            let mut config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            config.project_config = Some(ProjectLunaticConfig::default());
            config.flush()?;
        }
        ProjectArgs::Get { name, url, id } => {
            let config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            if name {
                info!("Name of project: {}", config.project_config()?.project_name);
            } else if url {
                info!(
                    "Url of project: {}/{}",
                    config.project_config()?.provider, config.project_config()?.project_url
                );
            } else if id {
                info!("Project ID: {}", config.project_config()?.project_id);
            }
        }
    }
    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}
