use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Read, Write},
    path::Path,
};

use anyhow::{anyhow, Context, Result};
use log::debug;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;
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
        let mut artefact = File::open(artefact)?;
        let mut artefact_bytes = Vec::new();
        artefact.read_to_end(&mut artefact_bytes)?;
        let new_version_id = config
            .upload_artefact_for_app(&app_id, artefact_bytes, binary_name)
            .await?;

        let (client, provider) = config.get_http_client()?;
        let root_url = provider.get_url()?;

        upload_env_vars_if_exist(&cwd, &client, &root_url, env_id).await?;
        upload_static_files_if_exist(&cwd, &client, &root_url, env_id).await?;

        let response = client
            .post(root_url.join(&format!("api/env/{}/start", env_id))?)
            .json(&StartApp { app_id })
            .send()
            .await
            .with_context(|| "Error sending HTTP app start request.")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.with_context(|| {
                format!("Error parsing body as text. Response not successful: {status}")
            })?;
            return Err(anyhow!(
                "HTTP start app request returned an error reponse: {body}"
            ));
        }
        println!(
            "Deployed project: {project_name} new version app \"{}\", version={new_version_id}",
            cargo.package.name
        );
        Ok(())
    } else {
        Err(anyhow!("Cannot find {binary_name} build directory"))
    }
}

// TODO: add ".env" file to config
async fn upload_env_vars_if_exist(
    cwd: &Path,
    client: &Client,
    root_url: &Url,
    env_id: i64,
) -> Result<()> {
    let mut envs = HashMap::new();
    let envs_path = cwd.join(".env");
    if envs_path.exists() && envs_path.is_file() {
        if let Ok(iter) = dotenvy::dotenv_iter() {
            for item in iter {
                let (key, val) = item.with_context(|| "Error reading .env variables.")?;
                envs.insert(key, val);
            }
            let response = client
                .post(root_url.join(&format!("api/env/{}/vars", env_id))?)
                .json(&envs)
                .send()
                .await
                .with_context(|| "Error sending HTTP post env vars request.")?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.with_context(|| {
                    format!("Error parsing body as text. Response not successfull: {status}")
                })?;
                return Err(anyhow!(
                    "HTTP post env vars request returned an error response: {body}"
                ));
            }
        }
    }
    Ok(())
}

// TODO: add "static" directory to config
async fn upload_static_files_if_exist(
    cwd: &Path,
    client: &Client,
    root_url: &Url,
    env_id: i64,
) -> Result<()> {
    let static_path = cwd.join("static");
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

        let response = client
            .post(root_url.join(&format!("api/env/{}/assets", env_id))?)
            .multipart(form)
            .send()
            .await
            .with_context(|| "Error sending HTTP post assets zip requests.")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.with_context(|| {
                format!("Error parsing body as text. Response not successfull: {status}")
            })?;
            return Err(anyhow!(
                "HTTP post assets zip request returned an error response: {body}"
            ));
        }
    }
    Ok(())
}
