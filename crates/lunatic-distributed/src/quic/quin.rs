use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use lunatic_process::{env::Environment, state::ProcessState};
use quinn::{ClientConfig, Connecting, ConnectionError, Endpoint, ServerConfig};
use rustls_pemfile::Item;
use wasmtime::ResourceLimiter;

use crate::{control, distributed, DistributedCtx};

pub struct SendStream {
    pub stream: quinn::SendStream,
}

impl SendStream {
    pub async fn send(&mut self, data: &mut [Bytes]) -> Result<()> {
        self.stream.write_all_chunks(data).await?;
        Ok(())
    }
}

pub struct RecvStream {
    pub stream: quinn::RecvStream,
}

impl RecvStream {
    pub async fn receive(&mut self) -> Result<Bytes> {
        let mut size = [0u8; 4];
        self.stream.read_exact(&mut size).await?;
        let size = u32::from_le_bytes(size);
        let mut buffer = vec![0u8; size as usize];
        self.stream.read_exact(&mut buffer).await?;
        Ok(buffer.into())
    }

    pub fn id(&self) -> quinn::StreamId {
        self.stream.id()
    }
}

#[derive(Clone)]
pub struct Client {
    inner: Endpoint,
}

impl Client {
    pub async fn connect(
        &self,
        addr: SocketAddr,
        name: &str,
        retry: u32,
    ) -> Result<(SendStream, RecvStream)> {
        for _ in 0..retry {
            let conn = self.inner.connect(addr, name)?.await?;
            if let Ok((send, recv)) = conn.open_bi().await {
                return Ok((SendStream { stream: send }, RecvStream { stream: recv }));
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

pub fn new_quic_server(addr: SocketAddr, cert: &str, key: &str) -> Result<Endpoint> {
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
    quic_server: &mut Endpoint,
    control_server: control::server::Server,
) -> Result<()> {
    while let Some(conn) = quic_server.accept().await {
        tokio::spawn(handle_quic_stream(conn, control_server.clone()));
    }
    Ok(())
}

async fn handle_quic_stream(
    conn: Connecting,
    control_server: control::server::Server,
) -> Result<()> {
    let conn = conn.await?;
    loop {
        let stream = conn.accept_bi().await;
        match stream {
            Ok((s, r)) => {
                let send = SendStream { stream: s };
                let recv = RecvStream { stream: r };
                tokio::spawn(handle_quic_connection(send, recv, control_server.clone()));
            }
            Err(ConnectionError::LocallyClosed) => {
                break;
            }
            Err(_) => {}
        }
    }
    Ok(())
}

async fn handle_quic_connection(
    mut send: SendStream,
    mut recv: RecvStream,
    control_server: control::server::Server,
) {
    while let Ok(bytes) = recv.receive().await {
        if let Ok((msg_id, request)) =
            bincode::deserialize::<(u64, control::message::Request)>(&bytes)
        {
            control::server::handle_request(control_server.clone(), &mut send, msg_id, request)
                .await
                .ok();
        }
    }
}

pub async fn handle_node_server<T, E>(
    quic_server: &mut Endpoint,
    ctx: distributed::server::ServerCtx<T, E>,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    while let Some(conn) = quic_server.accept().await {
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
    let conn = conn.await?;
    loop {
        let stream = conn.accept_bi().await;
        match stream {
            Ok((s, r)) => {
                let send = SendStream { stream: s };
                let recv = RecvStream { stream: r };
                tokio::spawn(handle_quic_stream_node(ctx.clone(), send, recv));
            }
            Err(ConnectionError::LocallyClosed) => break,
            Err(_) => {}
        }
    }
    Ok(())
}

async fn handle_quic_stream_node<T, E>(
    ctx: distributed::server::ServerCtx<T, E>,
    mut send: SendStream,
    mut recv: RecvStream,
) where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    while let Ok(bytes) = recv.receive().await {
        if let Ok((msg_id, request)) =
            bincode::deserialize::<(u64, distributed::message::Request)>(&bytes)
        {
            distributed::server::handle_message(ctx.clone(), &mut send, msg_id, request).await;
        } else {
            log::debug!("Error deserializing request");
        }
    }
}
