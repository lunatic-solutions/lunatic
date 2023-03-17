use anyhow::{anyhow, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

use super::config::ConfigManager;

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
    Remote {
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
            let config =
                ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;
            let app_list = config.list_project_apps().await?;
            println!("GOT THESE ACTIVE PROJECT APPS {app_list:?}");
            // provider.token;
            // reqwest::config.project_config.
        }
        AppArgs::Remote { name } => todo!(),
        AppArgs::List => todo!(),
    }
    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}
