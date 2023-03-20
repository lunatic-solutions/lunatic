use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use log::{debug, info};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::mode::config::{ConfigManager, Provider};

#[derive(Parser, Debug)]
pub(crate) struct Args {
    /// contains destination of auth server
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
    // pub user_id: String,
}

static CTRL_URL: &str = "http://localhost:3000";

pub(crate) async fn start(Args { provider }: Args) -> Result<()> {
    let mut config_manager = ConfigManager::new().unwrap();
    let base_url = provider.unwrap_or_else(|| CTRL_URL.to_string());
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/cli/login"))
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
    info!("\n\nPlease visit the following URL to authenticate this cli app {base_url}/cli/authenticate/{app_id}?login_id={login_id}\n\n");

    let status_url = format!("{base_url}/api/cli/login/{}", res.login_id);
    let auth_status = check_auth_status(&status_url, &client).await;
    if auth_status.is_empty() {
        panic!("Cli Login failed");
    }

    config_manager.add_provider(Provider {
        name: base_url,
        cookies: auth_status,
    });
    config_manager.flush()?;
    Ok(())
}

async fn check_auth_status(status_url: &str, client: &reqwest::Client) -> Vec<String> {
    loop {
        match client.get(status_url).send().await {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    return res
                        .cookies()
                        .map(|cookie| cookie.value().to_string())
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
