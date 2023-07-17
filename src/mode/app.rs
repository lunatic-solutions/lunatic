use anyhow::{anyhow, Context, Result};
use clap::Parser;
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
            let (client, provider) = config_manager.get_http_client()?;
            let url = provider.get_url()?;
            let response = client
                .post(url.join("api/projects")?)
                .json(&CreateProject { name })
                .send()
                .await
                .with_context(|| "Error sending HTTP create app request.")?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.with_context(|| {
                    format!("Error parsing body as text. Respose not successful: {status}")
                })?;
                return Err(anyhow!(
                    "HTTP create app request returned an error response: {body}"
                ));
            }
            let project = response
                .json::<Project>()
                .await
                .with_context(|| "Error parsing the create app request JSON.")?;
            let response = client
                .get(url.join(&format!("api/projects/{}", project.project_id))?)
                .send()
                .await
                .with_context(|| "Error sending HTTP get project request.")?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.with_context(|| {
                    format!("Error parsing body as text. Response not successful: {status}")
                })?;
                return Err(anyhow!(
                    "HTTP get project request returned an error response: {body}"
                ));
            }
            let project_details = response
                .json::<ProjectDetails>()
                .await
                .with_context(|| "Error parsing the get project request JSON.")?;
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
            });
            config_manager.flush()?;
        }
    }
    Ok(())
}
