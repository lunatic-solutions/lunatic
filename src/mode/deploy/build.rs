use std::process::Stdio;

use anyhow::{anyhow, Result};
use log::{debug, info};

use crate::mode::{
    config::{AppConfig, ConfigManager},
    deploy::artefact::get_target_dir,
};

use super::Args;

pub(crate) async fn start_build(args: Args) -> Result<Vec<AppConfig>> {
    let target_dir = get_target_dir();
    let config = ConfigManager::new().map_err(|e| anyhow!("failed to load config {e:?}"))?;

    let mut artefact_apps: Vec<AppConfig> = vec![];

    for app in config.project_config.remote.iter() {
        let mut build_args = vec!["build"];
        if !app.has_valid_mapping() {
            return Err(anyhow!("App {} has no valid mapping and cannot be deployed. Use `lunatic app add` to create mapping for App", app.app_name));
        }

        info!("Starting build of apps");
        if let Some(name) = args.name.as_deref() {
            if app.app_name == name {
                artefact_apps.push(app.to_owned());
                build_args.extend_from_slice(&app.get_build_flags());
                break;
            }
        } else if args.all {
            artefact_apps.push(app.to_owned().clone());
            build_args.extend_from_slice(&app.get_build_flags());
        }

        if args.release {
            build_args.push("--release");
        }

        debug!("Executing the command `cargo {:?}`", build_args.join(" "));
        std::process::Command::new("cargo")
            .env("CARGO_TARGET_DIR", target_dir.clone())
            .args(build_args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| anyhow!("failed to execute build {e:?}"))?;
    }

    info!("Successfully built artefacts");

    Ok(artefact_apps)
}
