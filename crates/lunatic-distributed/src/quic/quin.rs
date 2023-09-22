use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use dashmap::DashMap;
use lunatic_process::{env::Environment, state::ProcessState};
use quinn::{ClientConfig, Connecting, Connection, ConnectionError, Endpoint, ServerConfig};
use rustls::server::AllowAnyAuthenticatedClient;
use rustls_pemfile::Item;
use wasmtime::ResourceLimiter;
use x509_parser::{der_parser::oid, oid_registry::asn1_rs::Utf8String, prelude::FromDer};

use crate::{
    distributed::{self},
    DistributedCtx, CertAttrs,
};

#[derive(Clone)]
pub struct Client {
    inner: Endpoint,
}

impl Client {
    pub async fn _connect(&self, addr: SocketAddr, name: &str) -> Result<quinn::Connection> {
        Ok(self.inner.connect(addr, name)?.await?)
    }

    pub async fn try_connect(
        &self,
        addr: SocketAddr,
        name: &str,
        retry: u32,
    ) -> Result<quinn::Connection> {
        for try_num in 1..(retry + 1) {
            match self._connect(addr, name).await {
                Ok(conn) => return Ok(conn),
                Err(e) => {
                    log::error!("Error connecting to {name} at {addr}, try {try_num}. Error: {e}")
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(anyhow!("Failed to connect to {name} at {addr}"))
    }
}

fn get_cert_attrs(conn: &Connection) -> Result<CertAttrs> {
    let peer_identity = match conn
        .peer_identity()
        .ok_or(anyhow!("Peer must provide an identity."))?
        .downcast::<Vec<rustls::Certificate>>()
    {
        Ok(certs) => Ok(certs),
        Err(_) => Err(anyhow!("Failed to downcast peer identity.")),
    }?;
    if peer_identity.len() != 1 {
        return Err(anyhow!("More than one identity certificate detected."));
    }
    let cert = peer_identity.get(0).unwrap();
    let (_rem, x509) = x509_parser::certificate::X509Certificate::from_der(&cert.0)?;
    let oid = oid!(2.5.29 .9);
    let ext = x509
        .get_extension_unique(&oid)?
        .ok_or_else(|| anyhow!("Missing critical Lunatic certificate extension."))?;
    let (_rem, value) = Utf8String::from_der(ext.value)?;
    Ok(serde_json::from_str(&value.string())?)
}

pub fn new_quic_client(ca_cert: &str, cert: &str, key: &str) -> Result<Client> {
    let mut ca_cert = ca_cert.as_bytes();
    let ca_cert = rustls_pemfile::read_one(&mut ca_cert)?.unwrap();
    let ca_cert = match ca_cert {
        Item::X509Certificate(ca_cert) => Ok(rustls::Certificate(ca_cert)),
        _ => Err(anyhow!("Not a valid certificate.")),
    }?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(&ca_cert)?;

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

    let client_crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_client_auth_cert(cert, pk)?;

    let client_config = ClientConfig::new(Arc::new(client_crypto));
    let mut endpoint = Endpoint::client("[::]:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);
    Ok(Client { inner: endpoint })
}

pub fn new_quic_server(
    addr: SocketAddr,
    certs: Vec<String>,
    key: &str,
    ca_cert: &str,
) -> Result<Endpoint> {
    let mut ca_cert = ca_cert.as_bytes();
    let ca_cert = rustls_pemfile::read_one(&mut ca_cert)?.unwrap();
    let ca_cert = match ca_cert {
        Item::X509Certificate(ca_cert) => Ok(rustls::Certificate(ca_cert)),
        _ => Err(anyhow!("Not a valid certificate.")),
    }?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(&ca_cert)?;

    let mut key = key.as_bytes();
    let pk = rustls_pemfile::read_one(&mut key)?.unwrap();
    let pk = match pk {
        Item::PKCS8Key(key) => Ok(rustls::PrivateKey(key)),
        _ => Err(anyhow!("Not a valid private key.")),
    }?;

    let mut cert_chain = Vec::new();
    for cert in certs {
        let mut cert = cert.as_bytes();
        let cert = rustls_pemfile::read_one(&mut cert)?.unwrap();
        let cert = match cert {
            Item::X509Certificate(cert) => Ok(rustls::Certificate(cert)),
            _ => Err(anyhow!("Not a valid certificate")),
        }?;
        cert_chain.push(cert);
    }

    let server_crypto = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(Arc::new(AllowAnyAuthenticatedClient::new(roots)))
        .with_single_cert(cert_chain, pk)?;
    let mut server_config = ServerConfig::with_crypto(Arc::new(server_crypto));
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .keep_alive_interval(Some(Duration::from_millis(100)));

    Ok(quinn::Endpoint::server(server_config, addr)?)
}

pub async fn handle_node_server<T, E>(
    quic_server: &mut Endpoint,
    ctx: distributed::server::ServerCtx<T, E>,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + Sync + 'static,
    E: Environment + 'static,
{
    while let Some(conn) = quic_server.accept().await {
        tokio::spawn(handle_quic_connection_node(ctx.clone(), conn));
    }
    Err(anyhow!("Node server exited"))
}

pub struct NodeEnvPermission(pub Option<HashSet<u64>>);

impl NodeEnvPermission {
    fn new(cert_attrs: CertAttrs) -> Self {
        let some_set: Option<HashSet<u64>> = if cert_attrs.is_privileged {
            None
        } else {
            Some(cert_attrs.allowed_envs.into_iter().collect())
        };
        Self(some_set)
    }
}

async fn handle_quic_connection_node<T, E>(
    ctx: distributed::server::ServerCtx<T, E>,
    conn: Connecting,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + Sync + 'static,
    E: Environment + 'static,
{
    log::info!("New node connection");
    let conn = conn.await?;
    let node_cert_attrs = get_cert_attrs(&conn)?;
    let node_permissions = Arc::new(NodeEnvPermission::new(node_cert_attrs));
    log::info!("Remote {} connected", conn.remote_address());
    loop {
        if let Some(reason) = conn.close_reason() {
            log::info!("Connection {} is closed: {reason}", conn.remote_address());
            break;
        }
        let stream = conn.accept_uni().await;
        log::info!("Stream from remote {} accepted", conn.remote_address());
        match stream {
            Ok(recv) => {
                tokio::spawn(handle_quic_stream_node(
                    ctx.clone(),
                    recv,
                    node_permissions.clone(),
                ));
            }
            Err(ConnectionError::LocallyClosed) => {
                log::trace!("distributed::server::stream locally closed");
                break;
            }
            Err(_) => {}
        }
    }
    log::info!("Connection from remote {} closed", conn.remote_address());
    Ok(())
}

async fn handle_quic_stream_node<T, E>(
    ctx: distributed::server::ServerCtx<T, E>,
    recv: quinn::RecvStream,
    node_permissions: Arc<NodeEnvPermission>,
) where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + Sync + 'static,
    E: Environment + 'static,
{
    let mut recv_ctx = RecvCtx {
        recv,
        chunks: DashMap::new(),
    };
    log::trace!("distributed::server::handle_quic_stream started");
    while let Ok((msg_id, bytes)) = read_next_stream_message(&mut recv_ctx).await {
        if let Ok(request) = rmp_serde::from_slice::<distributed::message::Request>(&bytes) {
            distributed::server::handle_message(ctx.clone(), msg_id, request, node_permissions.clone())
                .await;
        } else {
            log::debug!("Error deserializing request");
        }
    }
    log::trace!("distributed::server::handle_quic_stream finished");
}

struct Chunk {
    message_id: u64,
    message_size: usize,
    data: Vec<u8>,
}

struct RecvCtx {
    recv: quinn::RecvStream,
    // Map to collect message chunks key: message_id, data: (message_size, data)
    chunks: DashMap<u64, (usize, Vec<u8>)>,
}

async fn read_next_stream_chunk(recv: &mut quinn::RecvStream) -> Result<Chunk> {
    // Read chunk header info
    let mut message_id = [0u8; 8];
    let mut message_size = [0u8; 4];
    let mut chunk_id = [0u8; 8];
    let mut chunk_size = [0u8; 4];
    recv.read_exact(&mut message_id)
        .await
        .map_err(|e| anyhow!("{e} failed to read header message_id"))?;
    recv.read_exact(&mut message_size)
        .await
        .map_err(|e| anyhow!("{e} failed to read header message_size"))?;
    recv.read_exact(&mut chunk_id)
        .await
        .map_err(|e| anyhow!("{e} failed to read header chunk_id"))?;
    recv.read_exact(&mut chunk_size)
        .await
        .map_err(|e| anyhow!("{e} failed to read header chunk_size"))?;
    let message_id = u64::from_le_bytes(message_id);
    let message_size = u32::from_le_bytes(message_size) as usize;
    let chunk_id = u64::from_le_bytes(chunk_id);
    let chunk_size = u32::from_le_bytes(chunk_size) as usize;
    // Read chunk data
    let mut data = vec![0u8; chunk_size];
    recv.read_exact(&mut data)
        .await
        .map_err(|e| anyhow!("{e} failed to read message body"))?;
    log::trace!("read message_id={message_id} chunk_id={chunk_id}");
    Ok(Chunk {
        message_id,
        message_size,
        data,
    })
}

async fn read_next_stream_message(ctx: &mut RecvCtx) -> Result<(u64, Bytes)> {
    loop {
        let new_chunk = read_next_stream_chunk(&mut ctx.recv).await?;
        let message_id = new_chunk.message_id;
        let message_size = new_chunk.message_size;
        if let Some(mut entry) = ctx.chunks.get_mut(&message_id) {
            entry.1.extend(new_chunk.data);
        } else {
            ctx.chunks
                .insert(message_id, (message_size, new_chunk.data));
        };
        let finished = ctx
            .chunks
            .get(&message_id)
            .map(|entry| entry.0 == entry.1.len());
        match finished {
            Some(true) => {
                let (message_id, data) = ctx.chunks.remove(&message_id).unwrap();
                log::trace!("Finished collecting message_id={message_id}");
                return Ok((message_id, Bytes::from(data.1)));
            }
            Some(false) => {
                continue;
            }
            None => unreachable!("Message must exists at all times"),
        }
    }
}
