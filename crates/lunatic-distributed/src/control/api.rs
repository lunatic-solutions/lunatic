use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr};

use crate::NodeInfo;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Register {
    pub node_name: uuid::Uuid,
    pub csr_pem: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registration {
    pub node_name: uuid::Uuid,
    pub cert_pem: String,
    pub authentication_token: String,
    pub root_cert: String,
    pub urls: ControlUrls,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlUrls {
    pub api_base: String,
    pub nodes: String,
    pub node_started: String,
    pub node_stopped: String,
    pub get_module: String,
    pub add_module: String,
    pub get_nodes: String,

    /// Get a process
    pub get_process: String,
    pub add_process: String,
    pub remove_process: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStart {
    pub node_address: SocketAddr,
    pub attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStarted {
    // TODO u64 ids should be JSON string but parsed into u64?
    pub node_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodesList {
    pub nodes: Vec<NodeInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// The binary for a wasm module.
pub struct ModuleBytes {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddModule {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleId {
    pub module_id: u64,
}
