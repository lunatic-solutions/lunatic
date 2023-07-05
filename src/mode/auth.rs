use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use log::debug;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::mode::config::{ConfigManager, Provider};

#[derive(Parser, Debug)]
pub struct Args {
    #[clap(short, long)]
    provider: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CliLoginResponse {
    pub login_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CliLogin {
    pub app_id: String,
}

static CTRL_URL: &str = "https://lunatic.cloud";

pub(crate) async fn start(args: Args) -> Result<()> {
    let mut config_manager = ConfigManager::new().unwrap();
    let provider = args.provider.unwrap_or_else(|| CTRL_URL.to_string());

    match config_manager.global_config.provider {
        Some(_) => {
            if dbg!(is_authenticated(&mut config_manager).await?) {
                println!("\n\nYou are already authenticated.\n\n");
                Ok(())
            } else {
                refresh_existing_login(&mut config_manager).await
            }
        }
        None => new_login(provider, &mut config_manager).await,
    }
}

async fn check_auth_status(status_url: &str, client: &reqwest::Client) -> Vec<String> {
    loop {
        match client.get(status_url).send().await {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    return res
                        .headers()
                        .get_all("set-cookie")
                        .into_iter()
                        .map(|header| {
                            header
                                .to_str()
                                .expect("Failed to get Cookie value")
                                .to_string()
                        })
                        .collect();
                }
                if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&res.status()) {
                    debug!("Retrying in 5 seconds");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                panic!("Something went wrong {:?}", res);
            }
            Err(e) => {
                // code 401 means the app is still unauthorized and needs to try later
                if let Some(StatusCode::FORBIDDEN) = e.status() {
                    debug!("Retrying in 5 seconds");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                // something must have gone with either the request or the users connection
                panic!("Connection error {:?}", e);
            }
        }
    }
}

async fn new_login(provider: String, config_manager: &mut ConfigManager) -> Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{provider}/api/cli/login"))
        .json(&CliLogin {
            app_id: config_manager.get_app_id(),
        })
        .send()
        .await
        .expect("failed to send cli/login request")
        .json::<CliLoginResponse>()
        .await
        .expect("failed to parse JSON from cli/login");

    let login_id =
        url::form_urlencoded::byte_serialize(res.login_id.as_bytes()).collect::<String>();
    let app_id = url::form_urlencoded::byte_serialize(config_manager.get_app_id().as_bytes())
        .collect::<String>();
    println!("\n\nPlease visit the following URL to authenticate this cli app {provider}/cli/authenticate/{app_id}?login_id={login_id}\n\n");

    let status_url = format!("{provider}/api/cli/login/{}", res.login_id);
    let auth_status = check_auth_status(&status_url, &client).await;

    if auth_status.is_empty() {
        panic!("Cli Login failed");
    }

    config_manager.login(Provider {
        name: provider,
        cookies: auth_status,
        login_id,
    });

    config_manager.flush()?;
    Ok(())
}

async fn is_authenticated(config_manager: &mut ConfigManager) -> Result<bool> {
    let provider = config_manager
        .global_config
        .provider
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Unexpected missing provider in `lunatic.toml`"))?;
    let client = reqwest::Client::new();
    match client
        .get(
            provider
                .get_url()?
                .join(&format!("api/cli/login/{}", provider.login_id))?,
        )
        .send()
        .await
    {
        Ok(res) => {
            if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&res.status()) {
                Ok(false)
            } else {
                Ok(true)
            }
        }
        Err(e) => {
            if let Some(StatusCode::UNAUTHORIZED)  = e.status() {
                Ok(false)
            } else {
                Ok(true)
            }
        }
    }
}

async fn refresh_existing_login(config_manager: &mut ConfigManager) -> Result<()> {
    let root = config_manager
        .global_config
        .provider
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Unexpected missing provider in `lunatic.toml`"))?
        .name
        .clone();
    let login_id = config_manager
        .global_config
        .provider
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Unexpected missing provider in `lunatic.toml`"))?
        .login_id
        .clone();
    let app_id = &config_manager.global_config.cli_app_id;
    println!("\n\nPlease visit the following URL to authenticate this cli app {root}/cli/refresh/{app_id}?login_id={login_id}\n\n");

    let client = reqwest::Client::new();
    let status_url = format!("{root}/api/cli/login/{}", login_id);
    let auth_status = check_auth_status(&status_url, &client).await;

    if auth_status.is_empty() {
        panic!("Cli Login failed");
    }

    config_manager.login(Provider {
        name: root,
        cookies: auth_status,
        login_id,
    });

    config_manager.flush()?;
    Ok(())
}
