use std::{fs::File, io::Read};

use anyhow::{anyhow, Result};
use log::{debug, info};
use serde::Deserialize;
mod artefact;
mod build;

use super::config::ConfigManager;

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Package,
}

pub(crate) async fn start() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut config = ConfigManager::new().map_err(|e| anyhow!("Failed to load config {e:?}"))?;
    let project_config = config
        .project_config
        .as_ref()
        .ok_or_else(|| anyhow!("Cannot find project config, missing `lunatic.toml`"))?;
    let project_name = project_config.project_name.clone();
    let app_id = project_config.app_id;

    let mut file = File::open(cwd.join("Cargo.toml")).map_err(|e| {
        anyhow!(
            "Cannot find project Cargo.toml in path {}. {e}",
            cwd.to_string_lossy()
        )
    })?;
    let mut content = String::new();

    file.read_to_string(&mut content)?;

    let cargo: CargoToml = toml::from_str(&content)?;
    debug!("{:#?}", cargo);

    build::start_build().await?;

    let binary_name = format!("{}.wasm", cargo.package.name);
    let artefact = cwd.join("target/wasm32-wasi/release").join(&binary_name);

    if artefact.exists() && artefact.is_file() {
        info!(
            "Deploying to Project {project_name} new version of app {}",
            cargo.package.name
        );
        let mut artefact = File::open(artefact)?;
        let mut artefact_bytes = Vec::new();
        artefact.read_to_end(&mut artefact_bytes)?;
        let new_version_id = config
            .upload_artefact_for_app(&app_id, artefact_bytes, binary_name)
            .await?;
        info!(
            "Deployed Project {project_name} new version app \"{}\", version={new_version_id}",
            cargo.package.name
        );
        Ok(())
    } else {
        Err(anyhow!("Cannot find {binary_name} build directory"))
    }
}
