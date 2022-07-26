use std::{
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use anyhow::Result;
use dashmap::DashMap;
use rcgen::*;

use crate::{
    connection::new_quic_server,
    control::message::{Request, Response},
};
use crate::{
    connection::Connection,
    control::message::{Registered, Registration},
};

#[derive(Clone)]
pub struct Server {
    inner: Arc<InnerServer>,
}

struct InnerServer {
    next_node_id: AtomicU64,
    nodes: DashMap<u64, Registration>,
    next_module_id: AtomicU64,
    modules: DashMap<u64, Vec<u8>>,
    ca_cert: Certificate,
}

impl Server {
    pub fn new(ca_cert: Certificate) -> Self {
        Self {
            inner: Arc::new(InnerServer {
                next_node_id: AtomicU64::new(1),
                next_module_id: AtomicU64::new(1),
                nodes: DashMap::new(),
                modules: DashMap::new(),
                ca_cert,
            }),
        }
    }

    pub fn next_node_id(&self) -> u64 {
        self.inner
            .next_node_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn next_module_id(&self) -> u64 {
        self.inner
            .next_module_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    fn register(&self, reg: Registration) -> Response {
        let node_id = self.next_node_id();
        let signed_cert = CertificateSigningRequest::from_pem(&reg.signing_request)
            .and_then(|sign_request| sign_request.serialize_pem_with_signer(&self.inner.ca_cert));
        match signed_cert {
            Ok(signed_cert) => {
                self.inner.nodes.insert(node_id, reg);
                Response::Register(Registered {
                    node_id,
                    signed_cert,
                })
            }
            Err(rcgen_err) => Response::Error(rcgen_err.to_string()),
        }
    }

    fn list_nodes(&self) -> Response {
        Response::Nodes(
            self.inner
                .nodes
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect(),
        )
    }

    fn add_module(&self, bytes: Vec<u8>) -> Response {
        let module_id = self.next_module_id();
        self.inner.modules.insert(module_id, bytes);
        Response::ModuleId(module_id)
    }

    fn get_module(&self, id: u64) -> Response {
        Response::Module(self.inner.modules.get(&id).map(|e| e.clone()))
    }
}

pub static CTRL_SERVER_NAME: &'static str = "ctrl.lunatic.cloud";
pub static TEST_ROOT_CERT: &'static str = include_str!("../../certs/root.pem");
static TEST_ROOT_KEYS: &'static str = include_str!("../../certs/root.keys.pem");

pub fn root_cert(
    test_ca: bool,
    ca_cert: Option<&str>,
    ca_keys: Option<&str>,
) -> Result<Certificate> {
    if test_ca {
        let key_pair = KeyPair::from_pem(TEST_ROOT_KEYS)?;
        let root_params = CertificateParams::from_ca_cert_pem(TEST_ROOT_CERT, key_pair)?;
        let root_cert = Certificate::from_params(root_params)?;
        Ok(root_cert)
    } else {
        let ca_cert_pem = std::fs::read(Path::new(
            ca_cert.ok_or_else(|| anyhow::anyhow!("Missing public root certificate."))?,
        ))?;
        let ca_keys_pem =
            std::fs::read(Path::new(ca_keys.ok_or_else(|| {
                anyhow::anyhow!("Missing public root certificate keys.")
            })?))?;
        let key_pair = KeyPair::from_pem(std::str::from_utf8(&ca_keys_pem)?)?;
        let root_params =
            CertificateParams::from_ca_cert_pem(std::str::from_utf8(&ca_cert_pem)?, key_pair)?;
        let root_cert = Certificate::from_params(root_params)?;
        Ok(root_cert)
    }
}

fn ctrl_cert() -> Result<Certificate> {
    let mut ctrl_params = CertificateParams::new(vec![CTRL_SERVER_NAME.into()]);
    ctrl_params
        .distinguished_name
        .push(DnType::OrganizationName, "Lunatic Inc.");
    ctrl_params
        .distinguished_name
        .push(DnType::CommonName, "Control CA");
    Ok(Certificate::from_params(ctrl_params)?)
}

fn default_server_certificates(root_cert: &Certificate) -> Result<(String, String)> {
    let ctrl_cert = ctrl_cert()?;
    let cert_pem = ctrl_cert.serialize_pem_with_signer(&root_cert)?;
    let key_pem = ctrl_cert.serialize_private_key_pem();
    Ok((cert_pem, key_pem))
}

pub async fn control_server(socket: SocketAddr, ca_cert: Certificate) -> Result<()> {
    let (cert_pem, key_pem) = default_server_certificates(&ca_cert)?;
    let mut quic_server = new_quic_server(socket, &cert_pem, &key_pem)?;
    let server = Server::new(ca_cert);
    while let Some(conn) = quic_server.accept().await {
        let addr = conn.remote_addr().unwrap();
        log::info!("New connection {addr}");
        tokio::task::spawn(handle_quic_connection(server.clone(), conn));
    }
    Ok(())
}

async fn handle_quic_connection(server: Server, mut conn: s2n_quic::Connection) {
    while let Ok(Some(stream)) = conn.accept_bidirectional_stream().await {
        tokio::spawn(handle_quic_stream(server.clone(), Connection::new(stream)));
    }
}

async fn handle_quic_stream(server: Server, conn: Connection) {
    while let Ok((msg_id, request)) = conn.receive::<Request>().await {
        tokio::spawn(handle_request(
            server.clone(),
            conn.clone(),
            msg_id,
            request,
        ));
    }
}

async fn handle_request(
    server: Server,
    conn: Connection,
    msg_id: u64,
    request: Request,
) -> Result<u64> {
    println!("HANDLE REQUEST {msg_id}: {}", request.kind());
    use crate::control::message::Request::*;
    let response = match request {
        Register(reg) => server.register(reg),
        ListNodes => server.list_nodes(),
        AddModule(bytes) => server.add_module(bytes),
        GetModule(id) => server.get_module(id),
    };
    conn.send(msg_id, response).await
}
