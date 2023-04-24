use anyhow::{anyhow, Result};
use clap::Parser;
use log::{debug, info};
mod artefact;
mod build;
use crate::mode::{config::AppConfig, deploy::artefact::find_compiled_binary};

pub(crate) use build::start_build;

use super::config::ConfigManager;

#[derive(Parser, Debug, Clone)]
pub(crate) struct Args {
    // /// Build and deploy the specified binary. This flag may be specified multiple times and supports common Unix glob patterns.
    // #[clap(short = 'd', long)]
    // bin: Option<Vec<String>>,

    // /// Build all binary targets.
    // #[clap(long)]
    // bins: bool,

    // /// Build and deploy the specified example. This flag may be specified multiple times and supports common Unix glob patterns.
    // #[clap(long)]
    // example: Option<Vec<String>>,

    // ///Build all example targets.
    // #[clap(long)]
    // examples: bool,
    name: Option<String>,

    /// Deploy all mapped apps
    #[clap(short = 'A', long)]
    all: bool,

    /// Build with release optimisations
    #[clap(short = 'r', long)]
    release: bool,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    let mut config = ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;

    let artefact_apps: Vec<AppConfig> = build::start_build(args.clone()).await?;
    config.update_project_apps().await?;
    debug!("Received the following artefact_apps {artefact_apps:?}");

    // collect artefacts
    for (artefact, app) in artefact_apps.into_iter().map(|app| {
        (
            find_compiled_binary(app.get_binary_name(), "wasm32-wasi", args.release),
            app,
        )
    }) {
        let app_id = config
            .upload_artefact_for_app(&app.app_id, artefact, app.get_binary_name())
            .await?;
        info!(
            "Uploaded app \"{}\". Created new version {app_id}",
            app.app_name
        );
    }

    Ok(())
}
