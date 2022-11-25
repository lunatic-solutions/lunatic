use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr};

use crate::NodeInfo;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Register {
    pub node_address: SocketAddr,
    pub node_name: uuid::Uuid,
    pub csr_pem: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    // TODO u64 ids should be JSON string but parsed into u64?
    pub node_id: i64,
    pub node_name: uuid::Uuid,
    pub cert_pem: String,
    pub authentication_token: String,
    pub root_cert: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStarted {
    pub node_id: i64,
    pub node_address: SocketAddr,
    pub node_name: uuid::Uuid,
    pub attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStartedResponse {
    pub node_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodesResponse {
    pub nodes: Vec<NodeInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleResponse {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddModule {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddModuleResponse {
    pub module_id: u64,
}
