pub mod api;

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: SocketAddr,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertAttrs {
    pub allowed_envs: Vec<u64>,
    pub is_privileged: bool,
}
