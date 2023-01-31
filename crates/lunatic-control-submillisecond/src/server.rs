use std::{
    net::{SocketAddr, TcpListener},
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lunatic::{
    abstract_process,
    process::{AbstractProcess, ProcessRef},
};
use lunatic_control::api::{NodeStart, Register};
use serde::{Deserialize, Serialize};
// use rcgen::Certificate;
use uuid::Uuid;

// use crate::routes;

pub struct ControlServer {
    // pub ca_cert: Certificate,
    // pub quic_client: lunatic_distributed::quic::Client,
    registrations: DashMap<u64, Registered>,
    nodes: DashMap<u64, NodeDetails>,
    modules: DashMap<u64, Vec<u8>>,
    next_registration_id: u64,
    next_node_id: u64,
    next_module_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registered {
    pub node_name: Uuid,
    pub csr_pem: String,
    pub cert_pem: String,
    pub authentication_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeDetails {
    pub registration_id: u64,
    pub status: i16,
    pub created_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub node_address: String,
    pub attributes: serde_json::Value,
}

impl ControlServer {
    pub fn lookup() -> Option<ProcessRef<Self>> {
        ProcessRef::lookup("ControlServer")
    }
}

#[abstract_process(visibility = pub)]
impl ControlServer {
    #[init]
    fn init(_: ProcessRef<Self>, _: ()) -> Self {
        Self::new()
    }

    pub fn new(// ca_cert: Certificate, quic_client: lunatic_distributed::quic::Client
    ) -> Self {
        Self {
            // ca_cert,
            // quic_client,
            registrations: DashMap::new(),
            nodes: DashMap::new(),
            modules: DashMap::new(),
            next_registration_id: 1,
            next_node_id: 1,
            next_module_id: 1,
        }
    }

    #[handle_message]
    pub fn register(&mut self, reg: Register, cert_pem: String, authentication_token: String) {
        let id = self.next_registration_id;
        self.next_registration_id += 1;
        let registered = Registered {
            node_name: reg.node_name,
            csr_pem: reg.csr_pem,
            cert_pem,
            authentication_token,
        };
        self.registrations.insert(id, registered);
    }

    #[handle_request]
    pub fn start_node(&mut self, registration_id: u64, data: NodeStart) -> (u64, String) {
        let id = self.next_node_id;
        self.next_node_id += 1;
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

    #[handle_message]
    pub fn stop_node(&self, reg_id: u64) {
        if let Some(mut node) = self.nodes.get_mut(&reg_id) {
            node.status = 2;
            node.stopped_at = Some(Utc::now());
        }
    }

    #[handle_request]
    pub fn add_module(&mut self, bytes: Vec<u8>) -> u64 {
        let id = self.next_module_id;
        self.next_module_id += 1;
        self.modules.insert(id, bytes);
        id
    }

    #[handle_request]
    pub fn get_nodes(&self) -> DashMap<u64, NodeDetails> {
        self.nodes.clone()
    }

    #[handle_request]
    pub fn get_registrations(&self) -> DashMap<u64, Registered> {
        self.registrations.clone()
    }

    #[handle_request]
    pub fn get_modules(&self) -> DashMap<u64, Vec<u8>> {
        self.modules.clone()
    }
}
