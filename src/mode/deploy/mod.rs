use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;
mod artefact;

use crate::mode::{
    config::AppConfig,
    deploy::artefact::{find_compiled_binary, get_target_dir},
};

use super::config::ConfigManager;

#[derive(Parser, Debug)]
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

    config.update_project_apps().await?;

    let mut build_args = vec!["build"];
    let mut artefact_apps: Vec<AppConfig> = vec![];

    for app in config.project_config.remote.iter() {
        if !app.has_valid_mapping() {
            return Err(anyhow!("App {} has no valid mapping and cannot be deployed. Use `lunatic app add` to create mapping for App", app.app_name));
        }

        if let Some(name) = args.name.as_deref() {
            if app.app_name == name {
                artefact_apps.push(app.to_owned());
                build_args.extend_from_slice(&app.get_build_flags());
                break;
            }
        } else if args.all && app.has_valid_mapping() {
            artefact_apps.push(app.to_owned().clone());
            build_args.extend_from_slice(&app.get_build_flags());
        }
    }

    if args.release {
        build_args.push("--release");
    }

    let build_result = std::process::Command::new("cargo")
        .env("CARGO_TARGET_DIR", get_target_dir())
        .args(build_args)
        .output()
        .map_err(|e| anyhow!("failed to execute build {e:?}"))?;

    // println!("{:?}", String::from_utf8(build_result.stdout));
    log::error!("Err {:?}", String::from_utf8(build_result.stderr));

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
