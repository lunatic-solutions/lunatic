use anyhow::Result;
use async_std::{
    net::{SocketAddr, TcpListener},
    task::spawn,
};
use lunatic_process::env::Environment;

use crate::{
    connection::Connection,
    distributed::message::{Request, Response},
};

pub async fn node_server(env: Environment, socket: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(socket).await?;
    while let Ok((conn, _addr)) = listener.accept().await {
        log::info!("New connection {_addr}");
        spawn(handle_connection(env.clone(), Connection::new(conn)));
    }
    Ok(())
}

async fn handle_connection(env: Environment, conn: Connection) {
    while let Ok((msg_id, msg)) = conn.receive::<Request>().await {
        spawn(handle_message(env.clone(), conn.clone(), msg_id, msg));
    }
}

async fn handle_message(
    _env: Environment,
    conn: Connection,
    msg_id: u64,
    msg: Request,
) -> Result<()> {
    match msg {
        Request::Spawn => {
            conn.send(msg_id, Response::Spawned).await?;
            log::info!("SPAWN")
        }
    }
    Ok(())
}
