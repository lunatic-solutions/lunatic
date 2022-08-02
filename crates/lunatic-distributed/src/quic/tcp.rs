use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use bincode::{deserialize, serialize};
use lunatic_process::state::ProcessState;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::Mutex,
};
use wasmtime::ResourceLimiter;

use crate::{control, distributed, DistributedCtx};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<OwnedReadHalf>,
    writer: Mutex<OwnedWriteHalf>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        Connection {
            inner: Arc::new(InnerConnection {
                reader: Mutex::new(read_half),
                writer: Mutex::new(write_half),
            }),
        }
    }

    pub async fn send<T: Serialize>(&self, msg_id: u64, msg: T) -> Result<u64> {
        let message = serialize(&(msg_id, msg))?;
        // Prefix message with size as little-endian u32 value.
        let size = (message.len() as u32).to_le_bytes();
        let mut writer = self.inner.writer.lock().await;
        writer.write_all(&size).await?;
        writer.write_all(&message).await?;
        Ok(msg_id)
    }

    pub async fn receive<T: DeserializeOwned>(&self) -> Result<(u64, T)> {
        let mut reader = self.inner.reader.lock().await;
        let mut size = [0u8; 4];
        reader.read_exact(&mut size).await?;
        let size = u32::from_le_bytes(size);
        let mut buffer = vec![0u8; size as usize];
        reader.read_exact(&mut buffer).await?;
        Ok(deserialize(&buffer)?)
    }
}

#[derive(Clone)]
pub struct Client;

impl Client {
    pub async fn connect(&self, addr: SocketAddr, _name: &str, retry: u32) -> Result<Connection> {
        for _ in 0..retry {
            log::info!("Connecting to control {addr}");
            if let Ok(stream) = TcpStream::connect(addr).await {
                return Ok(Connection::new(stream));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(anyhow!("Failed to connect to {addr}"))
    }
}

pub fn new_quic_client(_ca_cert: &str) -> Result<Client> {
    Ok(Client {})
}

#[derive(Clone)]
pub struct Server {
    addr: SocketAddr,
}

pub fn new_quic_server(addr: SocketAddr, _cert: &str, _key: &str) -> Result<Server> {
    Ok(Server { addr })
}

pub async fn handle_accept_control(
    quic_server: &mut Server,
    control_server: control::server::Server,
) -> Result<()> {
    let listener = TcpListener::bind(quic_server.addr).await?;
    while let Ok((conn, addr)) = listener.accept().await {
        log::info!("New connection {addr}");
        tokio::spawn(handle_quic_connection(
            control_server.clone(),
            Connection::new(conn),
        ));
    }
    Ok(())
}

async fn handle_quic_connection(server: control::server::Server, conn: Connection) {
    while let Ok((msg_id, request)) = conn.receive().await {
        tokio::spawn(control::server::handle_request(
            server.clone(),
            conn.clone(),
            msg_id,
            request,
        ));
    }
}

pub async fn handle_node_server<T>(
    quic_server: &mut Server,
    ctx: distributed::server::ServerCtx<T>,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    let listener = TcpListener::bind(quic_server.addr).await?;
    while let Ok((conn, addr)) = listener.accept().await {
        log::info!("New connection {addr}");
        tokio::task::spawn(handle_quic_connection_node(
            ctx.clone(),
            Connection::new(conn),
        ));
    }
    Ok(())
}

async fn handle_quic_connection_node<T>(
    ctx: distributed::server::ServerCtx<T>,
    conn: Connection,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    while let Ok((msg_id, request)) = conn.receive().await {
        distributed::server::handle_message(
            ctx.clone(),
            conn.clone(),
            msg_id,
            request,
        ).await;
    }
    Ok(())
}
