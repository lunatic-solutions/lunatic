use std::net::{SocketAddr, TcpListener};

use anyhow::{anyhow, Result};
use clap::Parser;

#[derive(Parser, Debug)]
pub(crate) struct Args {
    #[arg(long, value_name = "CONTROL_SERVER_SOCKET")]
    bind_socket: Option<SocketAddr>,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    if let Some(socket) = args.bind_socket {
        log::info!("Register URL: http://{}/", socket);
        lunatic_control_axum::server::control_server(socket).await?;
    } else if let Some(listener) = get_available_localhost() {
        log::info!("Register URL: http://{}/", listener.local_addr().unwrap());
        lunatic_control_axum::server::control_server_from_tcp(listener).await?;
    }

    Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}

fn get_available_localhost() -> Option<TcpListener> {
    for port in 3030..3999u16 {
        if let Ok(s) = TcpListener::bind(("127.0.0.1", port)) {
            return Some(s);
        }
    }

    for port in 1025..65535u16 {
        if let Ok(s) = TcpListener::bind(("127.0.0.1", port)) {
            return Some(s);
        }
    }

    None
}
