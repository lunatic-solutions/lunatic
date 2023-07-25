use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use log::debug;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};
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

#[derive(Debug, Serialize)]
struct StartApp {
    app_id: i64,
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
    let env_id = project_config.env_id;
    let env_vars = project_config.env_vars.clone();
    let assets_dir = project_config.assets_dir.clone();

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
        println!(
            "Deploying project: {project_name} new version of app {}",
            cargo.package.name
        );
        let new_version_id = upload_wasm_binary(app_id, binary_name, artefact, &mut config).await?;
        upload_env_vars_if_exist(&cwd, env_id, env_vars, &config).await?;
        upload_static_files_if_exist(&cwd, env_id, assets_dir, &config).await?;
        start_app(app_id, env_id, &config).await?;
        println!(
            "Deployed project: {project_name} new version app \"{}\", version={new_version_id}",
            cargo.package.name
        );
        Ok(())
    } else {
        Err(anyhow!("Cannot find {binary_name} build directory"))
    }
}

async fn upload_env_vars_if_exist(
    cwd: &Path,
    env_id: i64,
    env_vars: Option<String>,
    config_manager: &ConfigManager,
) -> Result<()> {
    let mut envs = HashMap::new();
    let envs_path = cwd.join(env_vars.unwrap_or_else(|| ".env".to_string()));
    if envs_path.exists() && envs_path.is_file() {
        if let Ok(iter) = dotenvy::from_path_iter(envs_path) {
            for item in iter {
                let (key, val) = item.with_context(|| "Error reading .env variables.")?;
                envs.insert(key, val);
            }
            config_manager
                .request_platform::<Value, HashMap<String, String>>(
                    Method::POST,
                    &format!("api/env/{}/vars", env_id),
                    "env vars",
                    Some(envs),
                    None,
                )
                .await?;
        }
    }
    Ok(())
}

async fn upload_wasm_binary(
    app_id: i64,
    binary_name: String,
    artefact: PathBuf,
    config_manager: &mut ConfigManager,
) -> Result<i64> {
    let mut artefact = File::open(artefact)?;
    let mut artefact_bytes = Vec::new();
    artefact.read_to_end(&mut artefact_bytes)?;
    let new_version_id = config_manager
        .upload_artefact_for_app(&app_id, artefact_bytes, binary_name)
        .await?;
    Ok(new_version_id)
}

async fn upload_static_files_if_exist(
    cwd: &Path,
    env_id: i64,
    assets_dir: Option<String>,
    config_manager: &ConfigManager,
) -> Result<()> {
    let static_path = cwd.join(assets_dir.unwrap_or_else(|| "static".to_string()));
    if static_path.exists() && static_path.is_dir() {
        let writer = Cursor::new(Vec::new());
        let options = FileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o755);
        let mut zip = ZipWriter::new(writer);
        let walkdir = walkdir::WalkDir::new(static_path.clone());
        let it = walkdir.into_iter();

        for entry in it {
            let entry = entry?;
            let path = entry.path();
            let name = path.strip_prefix(&static_path)?;

            if path.is_file() {
                zip.start_file(name.to_string_lossy().to_string(), options)?;
                let mut f = File::open(path)?;
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer)?;
                zip.write_all(&buffer)?;
            }
        }

        let buffer = zip.finish()?.into_inner();
        let part = reqwest::multipart::Part::bytes(buffer)
            .file_name("assets.zip")
            .mime_str("application/zip")?;
        let form = reqwest::multipart::Form::new().part("file", part);

        config_manager
            .request_platform::<Value, ()>(
                Method::POST,
                &format!("api/env/{}/assets", env_id),
                "assets zip",
                None,
                Some(form),
            )
            .await?;
    }
    Ok(())
}

async fn start_app(app_id: i64, env_id: i64, config_manager: &ConfigManager) -> Result<()> {
    config_manager
        .request_platform::<Value, StartApp>(
            Method::POST,
            &format!("api/env/{}/start", env_id),
            "app start",
            Some(StartApp { app_id }),
            None,
        )
        .await?;
    Ok(())
}
