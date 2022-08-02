use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};

use bincode::{deserialize, serialize};
use lunatic_process::state::ProcessState;
use s2n_quic::{
    client::Connect,
    stream::{BidirectionalStream, ReceiveStream, SendStream},
    Server,
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};
use wasmtime::ResourceLimiter;

use crate::{
    control::{self},
    distributed, DistributedCtx,
};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<ReceiveStream>,
    writer: Mutex<SendStream>,
}

impl Connection {
    pub fn new(stream: BidirectionalStream) -> Self {
        let (read_half, write_half) = stream.split();
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
pub struct Client {
    inner: s2n_quic::Client,
}

impl Client {
    pub async fn connect(&self, addr: SocketAddr, name: &str, retry: u32) -> Result<Connection> {
        for _ in 0..retry {
            log::info!("Connecting to control {addr}");
            let connect = Connect::new(addr).with_server_name(name);
            if let Ok(mut conn) = self.inner.connect(connect).await {
                conn.keep_alive(true)?;
                let stream = conn.open_bidirectional_stream().await?;
                return Ok(Connection::new(stream));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(anyhow!("Failed to connect to {addr}"))
    }
}

pub fn new_quic_client(ca_cert: &str) -> Result<Client> {
    match s2n_quic::Client::builder()
        .with_tls(ca_cert)?
        .with_io("0.0.0.0:0")?
        .start()
        .map_err(|_| anyhow::anyhow!("Failed to start QUIC client."))
    {
        Ok(client) => Ok(Client { inner: client }),
        Err(e) => Err(e),
    }
}

pub fn new_quic_server(addr: SocketAddr, cert: &str, key: &str) -> Result<Server> {
    Server::builder()
        .with_tls((cert, key))?
        .with_io(addr)?
        .start()
        .map_err(|_| anyhow::anyhow!("Failed to start QUIC server."))
}

pub async fn handle_accept_control(
    quic_server: &mut Server,
    control_server: control::server::Server,
) -> Result<()> {
    while let Some(conn) = quic_server.accept().await {
        let addr = conn.remote_addr()?;
        log::info!("New connection {addr}");
        tokio::spawn(handle_quic_connection(control_server.clone(), conn));
    }
    Ok(())
}

async fn handle_quic_connection(server: control::server::Server, mut conn: s2n_quic::Connection) {
    while let Ok(Some(stream)) = conn.accept_bidirectional_stream().await {
        tokio::spawn(handle_quic_stream(server.clone(), Connection::new(stream)));
    }
}

async fn handle_quic_stream(server: control::server::Server, conn: Connection) {
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
    while let Some(connection) = quic_server.accept().await {
        let addr = connection.remote_addr()?;
        log::info!("New connection {addr}");
        tokio::task::spawn(handle_quic_connection_node(ctx.clone(), connection));
    }
    Ok(())
}

async fn handle_quic_connection_node<T>(
    ctx: distributed::server::ServerCtx<T>,
    mut conn: s2n_quic::Connection,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    while let Ok(Some(stream)) = conn.accept_bidirectional_stream().await {
        tokio::spawn(handle_quic_stream_node(
            ctx.clone(),
            Connection::new(stream),
        ));
    }
    Ok(())
}

async fn handle_quic_stream_node<T>(ctx: distributed::server::ServerCtx<T>, conn: Connection)
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
}
