use std::{
    net::{SocketAddr, TcpListener},
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use anyhow::Result;
use axum::{Extension, Router};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lunatic_control::api::{NodeStart, Register};
use rcgen::Certificate;
use uuid::Uuid;

use crate::routes;

pub struct ControlServer {
    pub ca_cert: Certificate,
    pub quic_client: lunatic_distributed::quic::Client,
    pub registrations: DashMap<u64, Registered>,
    pub nodes: DashMap<u64, NodeDetails>,
    pub modules: DashMap<u64, Vec<u8>>,
    next_registration_id: AtomicU64,
    next_node_id: AtomicU64,
    next_module_id: AtomicU64,
}

#[derive(Clone)]
pub struct Registered {
    pub node_name: Uuid,
    pub csr_pem: String,
    pub cert_pem: String,
    pub authentication_token: String,
}

pub struct NodeDetails {
    pub registration_id: u64,
    pub status: i16,
    pub created_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub node_address: String,
    pub attributes: serde_json::Value,
}

impl ControlServer {
    pub fn new(ca_cert: Certificate, quic_client: lunatic_distributed::quic::Client) -> Self {
        Self {
            ca_cert,
            quic_client,
            registrations: DashMap::new(),
            nodes: DashMap::new(),
            modules: DashMap::new(),
            next_registration_id: AtomicU64::new(1),
            next_node_id: AtomicU64::new(1),
            next_module_id: AtomicU64::new(1),
        }
    }

    pub fn register(&self, reg: &Register, cert_pem: &str, authentication_token: &str) {
        let id = self
            .next_registration_id
            .fetch_add(1, atomic::Ordering::Relaxed);
        let registered = Registered {
            node_name: reg.node_name,
            csr_pem: reg.csr_pem.clone(),
            cert_pem: cert_pem.to_owned(),
            authentication_token: authentication_token.to_owned(),
        };
        self.registrations.insert(id, registered);
    }

    pub fn start_node(&self, registration_id: u64, data: NodeStart) -> (u64, String) {
        let id = self.next_node_id.fetch_add(1, atomic::Ordering::Relaxed);
        let details = NodeDetails {
            registration_id,
            status: 0,
            created_at: Utc::now(),
            stopped_at: None,
            node_address: data.node_address.to_string(),
            attributes: serde_json::json!(data.attributes),
        };
        self.nodes.insert(id, details);
        (id, data.node_address.to_string())
    }

    pub fn stop_node(&self, reg_id: u64) {
        if let Some(mut node) = self.nodes.get_mut(&reg_id) {
            node.status = 2;
            node.stopped_at = Some(Utc::now());
        }
    }

    pub fn add_module(&self, bytes: Vec<u8>) -> u64 {
        let id = self.next_module_id.fetch_add(1, atomic::Ordering::Relaxed);
        self.modules.insert(id, bytes);
        id
    }
}

fn prepare_app() -> Result<Router> {
    let ca_cert_str = lunatic_distributed::distributed::server::test_root_cert();
    let ca_cert = lunatic_distributed::control::cert::test_root_cert()?;
    let (ctrl_cert, ctrl_pk) =
        lunatic_distributed::control::cert::default_server_certificates(&ca_cert)?;
    let quic_client =
        lunatic_distributed::quic::new_quic_client(&ca_cert_str, &ctrl_cert, &ctrl_pk)?;
    let control = Arc::new(ControlServer::new(ca_cert, quic_client));
    let app = Router::new()
        .nest("/", routes::init_routes())
        .layer(Extension(control));
    Ok(app)
}

pub async fn control_server(http_socket: SocketAddr) -> Result<()> {
    control_server_from_tcp(TcpListener::bind(http_socket)?).await
}

pub async fn control_server_from_tcp(listener: TcpListener) -> Result<()> {
    let app = prepare_app()?;

    axum::Server::from_tcp(listener)?
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
