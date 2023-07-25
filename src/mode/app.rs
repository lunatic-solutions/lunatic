use anyhow::{anyhow, Result};
use clap::Parser;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::mode::config::ProjectLunaticConfig;

use super::config::ConfigManager;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum AppArgs {
    Create { name: String },
}

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    app: AppArgs,
}

#[derive(Serialize)]
pub struct CreateProject {
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Project {
    pub project_id: i64,
    pub name: String,
    pub domains: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct App {
    app_id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Env {
    env_id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProjectDetails {
    pub project_id: i64,
    pub apps: Vec<App>,
    pub envs: Vec<Env>,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    match args.app {
        AppArgs::Create { name } => {
            let mut config_manager = ConfigManager::new().unwrap();
            if config_manager.project_config.is_some() {
                return Err(anyhow!(
                    "Project is already initialized, `lunatic.toml` exists in current directory."
                ));
            }
            let (_, project): (StatusCode, Project) = config_manager
                .request_platform(
                    Method::POST,
                    "api/projects",
                    "create app",
                    Some(CreateProject { name }),
                    None,
                )
                .await?;
            let (_, project_details): (StatusCode, ProjectDetails) = config_manager
                .request_platform::<ProjectDetails, ()>(
                    Method::GET,
                    &format!("api/projects/{}", project.project_id),
                    "get project",
                    None,
                    None,
                )
                .await?;
            // TODO for now every project has single app and env
            config_manager.init_project(ProjectLunaticConfig {
                project_id: project.project_id,
                project_name: project.name,
                domains: project.domains,
                app_id: project_details
                    .apps
                    .get(0)
                    .map(|app| app.app_id)
                    .ok_or_else(|| anyhow!("Unexpected config missing app_id"))?,
                env_id: project_details
                    .envs
                    .get(0)
                    .map(|env| env.env_id)
                    .ok_or_else(|| anyhow!("Unexpected config missing env_id"))?,
                env_vars: None,
                assets_dir: None,
            });
            config_manager.flush()?;
        }
    }
    Ok(())
}
