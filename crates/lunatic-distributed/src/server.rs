use anyhow::Result;
use async_std::{
    net::{SocketAddr, TcpListener},
    task::spawn,
};

use crate::{
    connection::Connection,
    message::{Request, Response},
    node::Node,
};

pub async fn node_server(node: Node, socket: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(socket).await?;
    while let Ok((conn, _addr)) = listener.accept().await {
        log::info!("New connection {_addr}");
        spawn(handle_connection(node.clone(), Connection::new(conn)));
    }
    Ok(())
}

async fn handle_connection(node: Node, conn: Connection) {
    while let Ok((msg_id, msg)) = conn.receive::<Request>().await {
        spawn(handle_message(node.clone(), conn.clone(), msg_id, msg));
    }
}

async fn handle_message(_node: Node, conn: Connection, msg_id: u64, msg: Request) -> Result<()> {
    match msg {
        Request::Spawn => {
            conn.send(msg_id, Response::Spawned).await?;
            log::info!("SPAWN")
        }
    }
    Ok(())
}
