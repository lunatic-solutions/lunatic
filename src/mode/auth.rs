use std::{
    net::{SocketAddr, TcpListener},
    time::Duration,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use reqwest::{header::HeaderName, StatusCode};
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
    println!("Checking authentication for {provider:?}");
    let mut config_manager = ConfigManager::new().unwrap();
    let base_url = provider.unwrap_or_else(|| CTRL_URL.to_string());
    let client = reqwest::Client::new();
    println!("SENDING REQUEST {}", format!("{base_url}/api/cli/login"));
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

    println!("GOT RES {res:?}");
    let login_id =
        url::form_urlencoded::byte_serialize(res.login_id.as_bytes()).collect::<String>();
    let app_id = url::form_urlencoded::byte_serialize(config_manager.get_app_id().as_bytes())
        .collect::<String>();
    println!("Please visit the following URL to authenticate this cli app {base_url}/cli/authenticate/{app_id}?login_id={login_id}");

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
        println!("IN LOOP");
        match client.get(status_url).send().await {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    println!(
                        "Successfully authenticated cli app for {status_url} {res:?} {:?}",
                        res.headers()
                            .iter()
                            .filter(|h| h.0 == HeaderName::from_static("set-cookie"))
                            .map(|c| c.1.to_str().unwrap().to_string())
                            .collect::<Vec<String>>()
                    );
                    return res
                        .cookies()
                        .map(|cookie| cookie.value().to_string())
                        .collect();
                }
                if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&res.status()) {
                    // TODO: add debug log
                    println!("Sleeping for 5 seconds");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                println!("GOT RES {res:?}");
                panic!("Something went wrong {:?}", res);
            }
            Err(e) => {
                println!("ERROR HAPPENED DURING CALL {e:?}");
                // code 401 means the app is still unauthorized and needs to try later
                if let Some(StatusCode::FORBIDDEN) = e.status() {
                    // TODO: add debug log
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                // something must have gone with either the request or the users connection
                panic!("Connection error {e:?}");
            }
        }
    }
}

// Definition for singly-linked list.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListNode {
    pub val: i32,
    pub next: Option<Box<ListNode>>,
}

impl ListNode {
    #[inline]
    fn new(val: i32) -> Self {
        ListNode { next: None, val }
    }
}
