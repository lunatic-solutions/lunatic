use std::{
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use crate::{control::message::Response, NodeInfo};
use crate::{
    control::message::{Registered, Registration},
    quic::SendStream,
};
use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use rcgen::*;

use super::parser::Parser;

#[derive(Clone)]
pub struct Server {
    inner: Arc<InnerServer>,
}

struct InnerServer {
    next_node_id: AtomicU64,
    nodes: DashMap<u64, Registration>,
    addr_to_node: DashMap<SocketAddr, u64>,
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
                addr_to_node: DashMap::new(),
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

    pub fn register(&self, reg: Registration) -> Response {
        let node_id = self.next_node_id();
        let signed_cert = CertificateSigningRequest::from_pem(&reg.signing_request)
            .and_then(|sign_request| sign_request.serialize_pem_with_signer(&self.inner.ca_cert));
        match signed_cert {
            Ok(signed_cert) => {
                // Remove another node using the same address. This is temporarily until we define
                // details of connection status & reconnecting/registering.
                if let Some(proc_id) = self.inner.addr_to_node.get(&reg.node_address) {
                    self.inner.nodes.remove(&proc_id);
                }

                self.inner.addr_to_node.insert(reg.node_address, node_id);
                self.inner.nodes.insert(node_id, reg);

                Response::Register(Registered {
                    node_id,
                    signed_cert,
                })
            }
            Err(rcgen_err) => Response::Error(rcgen_err.to_string()),
        }
    }

    pub fn deregister(&self, node_id: u64) -> Response {
        self.inner.nodes.remove(&node_id);
        Response::None
    }

    pub fn list_nodes(&self) -> Response {
        Response::Nodes(
            self.inner
                .nodes
                .iter()
                .map(|e| NodeInfo {
                    id: *e.key(),
                    address: e.value().node_address,
                    name: e.value().node_name.clone(),
                })
                .collect(),
        )
    }

    pub fn lookup_nodes(&self, query: String) -> Response {
        let parser = Parser::new(query);
        match parser.parse() {
            Ok(filter) => Response::Nodes(
                self.inner
                    .nodes
                    .iter()
                    .filter(|e| filter.apply(e))
                    .map(|e| NodeInfo {
                        id: *e.key(),
                        address: e.node_address,
                        name: e.node_name.clone(),
                    })
                    .collect(),
            ),
            Err(e) => Response::Error(e.to_string()),
        }
    }

    pub fn add_module(&self, bytes: Vec<u8>) -> Response {
        let module_id = self.next_module_id();
        self.inner.modules.insert(module_id, bytes);
        Response::ModuleId(module_id)
    }

    pub fn get_module(&self, id: u64) -> Response {
        Response::Module(self.inner.modules.get(&id).map(|e| e.clone()))
    }
}

pub static CTRL_SERVER_NAME: &str = "ctrl.lunatic.cloud";
pub static TEST_ROOT_CERT: &str = r#"""
-----BEGIN CERTIFICATE-----
MIIBnDCCAUGgAwIBAgIIR5Hk+O5RdOgwCgYIKoZIzj0EAwIwKTEQMA4GA1UEAwwH
Um9vdCBDQTEVMBMGA1UECgwMTHVuYXRpYyBJbmMuMCAXDTc1MDEwMTAwMDAwMFoY
DzQwOTYwMTAxMDAwMDAwWjApMRAwDgYDVQQDDAdSb290IENBMRUwEwYDVQQKDAxM
dW5hdGljIEluYy4wWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAARlVNxYAwsmmFNc
2EMBbZZVwL8GBtnnu8IROdDd68ixc0VBjfrV0zAM344lKJcs9slsMTEofoYvMCpI
BhnSGyAFo1EwTzAdBgNVHREEFjAUghJyb290Lmx1bmF0aWMuY2xvdWQwHQYDVR0O
BBYEFOh0Ue745JFH76xErjqkW2/SbHhAMA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZI
zj0EAwIDSQAwRgIhAJKPv4XUZ9ej+CVgsJ+9x/CmJEcnebyWh2KntJri97nxAiEA
/KvaQE6GtYZPGFv/WYM3YEmTQ7hoOvaaAuvD27cHkaw=
-----END CERTIFICATE-----
"""#;
static TEST_ROOT_KEYS: &str = r#"""
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg9ferf0du4h975Jhu
boMyGfdI+xwp7ewOulGvpTcvdpehRANCAARlVNxYAwsmmFNc2EMBbZZVwL8GBtnn
u8IROdDd68ixc0VBjfrV0zAM344lKJcs9slsMTEofoYvMCpIBhnSGyAF
-----END PRIVATE KEY-----"""#;

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
            ca_cert.ok_or_else(|| anyhow::anyhow!("Missing CA certificate."))?,
        ))?;
        let ca_keys_pem = std::fs::read(Path::new(
            ca_keys.ok_or_else(|| anyhow::anyhow!("Missing CA keys."))?,
        ))?;
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
    let cert_pem = ctrl_cert.serialize_pem_with_signer(root_cert)?;
    let key_pem = ctrl_cert.serialize_private_key_pem();
    Ok((cert_pem, key_pem))
}

pub async fn control_server(socket: SocketAddr, ca_cert: Certificate) -> Result<()> {
    let (cert_pem, key_pem) = default_server_certificates(&ca_cert)?;
    let mut quic_server = crate::quic::new_quic_server(socket, &cert_pem, &key_pem)?;
    let server = Server::new(ca_cert);
    crate::quic::handle_accept_control(&mut quic_server, server.clone()).await?;
    Ok(())
}

pub async fn handle_request(
    server: Server,
    send: &mut SendStream,
    msg_id: u64,
    request: crate::control::message::Request,
) -> Result<u64> {
    use crate::control::message::Request::*;
    let response = match request {
        Register(reg) => server.register(reg),
        Deregister(node_id) => server.deregister(node_id),
        ListNodes => server.list_nodes(),
        AddModule(bytes) => server.add_module(bytes),
        GetModule(id) => server.get_module(id),
        LookupNodes(query) => server.lookup_nodes(query),
    };
    let data = bincode::serialize(&(msg_id, response))?;
    let size = (data.len() as u32).to_le_bytes();
    let size: Bytes = Bytes::copy_from_slice(&size[..]);
    let bytes: Bytes = data.into();
    send.send(&mut [size, bytes]).await?;
    Ok(msg_id)
}
