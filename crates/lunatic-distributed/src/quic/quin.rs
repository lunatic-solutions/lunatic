use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use bincode::{deserialize, serialize};
use futures_util::StreamExt;
use lunatic_process::{env::Environment, state::ProcessState};
use quinn::{
    ClientConfig, Connecting, Endpoint, Incoming, NewConnection, RecvStream, SendStream,
    ServerConfig,
};
use rustls_pemfile::Item;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::Mutex;
use wasmtime::ResourceLimiter;

use crate::{control, distributed, DistributedCtx};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<RecvStream>,
    writer: Mutex<SendStream>,
}

impl Connection {
    pub fn new(stream: (SendStream, RecvStream)) -> Self {
        let (write_half, read_half) = stream;
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
    inner: Endpoint,
}

impl Client {
    pub async fn connect(&self, addr: SocketAddr, name: &str, retry: u32) -> Result<Connection> {
        for _ in 0..retry {
            log::info!("Connecting to control {addr}");
            let new_conn = self.inner.connect(addr, name)?.await?;
            let NewConnection {
                connection: conn, ..
            } = new_conn;
            if let Ok(stream) = conn.open_bi().await {
                return Ok(Connection::new(stream));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(anyhow!("Failed to connect to {addr}"))
    }
}

pub fn new_quic_client(ca_cert: &str) -> Result<Client> {
    let mut cert = ca_cert.as_bytes();
    let cert = rustls_pemfile::read_one(&mut cert)?.unwrap();
    let cert = match cert {
        Item::X509Certificate(cert) => Ok(rustls::Certificate(cert)),
        _ => Err(anyhow!("Not a valid certificate.")),
    }?;
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&cert)?;
    let client_crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(certs)
        .with_no_client_auth();
    let client_config = ClientConfig::new(Arc::new(client_crypto));
    let mut endpoint = Endpoint::client("[::]:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);
    Ok(Client { inner: endpoint })
}

pub fn new_quic_server(addr: SocketAddr, cert: &str, key: &str) -> Result<(Endpoint, Incoming)> {
    let mut cert = cert.as_bytes();
    let mut key = key.as_bytes();
    let pk = rustls_pemfile::read_one(&mut key)?.unwrap();
    let pk = match pk {
        Item::PKCS8Key(key) => Ok(rustls::PrivateKey(key)),
        _ => Err(anyhow!("Not a valid private key.")),
    }?;
    let cert = rustls_pemfile::read_one(&mut cert)?.unwrap();
    let cert = match cert {
        Item::X509Certificate(cert) => Ok(rustls::Certificate(cert)),
        _ => Err(anyhow!("Not a valid certificate")),
    }?;
    let cert = vec![cert];
    let server_crypto = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert, pk)?;
    let mut server_config = ServerConfig::with_crypto(Arc::new(server_crypto));
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .max_concurrent_uni_streams(0_u8.into());

    Ok(quinn::Endpoint::server(server_config, addr)?)
}

pub async fn handle_accept_control(
    quic_server: &mut (Endpoint, Incoming),
    control_server: control::server::Server,
) -> Result<()> {
    while let Some(conn) = quic_server.1.next().await {
        tokio::spawn(handle_quic_stream(conn, control_server.clone()));
    }
    Ok(())
}

async fn handle_quic_stream(
    conn: Connecting,
    control_server: control::server::Server,
) -> Result<()> {
    let NewConnection { mut bi_streams, .. } = conn.await?;
    while let Some(stream) = bi_streams.next().await {
        let connection = Connection::new(stream?);
        tokio::spawn(handle_quic_connection(
            connection.clone(),
            control_server.clone(),
        ));
    }
    Ok(())
}

async fn handle_quic_connection(connection: Connection, control_server: control::server::Server) {
    while let Ok((msg_id, request)) = connection.receive().await {
        tokio::spawn(control::server::handle_request(
            control_server.clone(),
            connection.clone(),
            msg_id,
            request,
        ));
    }
}

pub async fn handle_node_server<T, E>(
    quic_server: &mut (Endpoint, Incoming),
    ctx: distributed::server::ServerCtx<T, E>,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    while let Some(conn) = quic_server.1.next().await {
        tokio::spawn(handle_quic_connection_node(ctx.clone(), conn));
    }
    Ok(())
}

async fn handle_quic_connection_node<T, E>(
    ctx: distributed::server::ServerCtx<T, E>,
    conn: Connecting,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    let NewConnection { mut bi_streams, .. } = conn.await?;
    while let Some(stream) = bi_streams.next().await {
        let connection = Connection::new(stream?);
        tokio::spawn(handle_quic_stream_node(ctx.clone(), connection));
    }
    Ok(())
}

async fn handle_quic_stream_node<T, E>(ctx: distributed::server::ServerCtx<T, E>, conn: Connection)
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    while let Ok((msg_id, request)) = conn.receive().await {
        distributed::server::handle_message(ctx.clone(), conn.clone(), msg_id, request).await;
    }
}
