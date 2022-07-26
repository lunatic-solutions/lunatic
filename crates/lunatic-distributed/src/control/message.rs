use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Register(Registration),
    ListNodes,
    AddModule(Vec<u8>),
    GetModule(u64),
}

impl Request {
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Register(_) => "Register",
            Request::ListNodes => "ListNodes",
            Request::AddModule(_) => "AddModule",
            Request::GetModule(_) => "GetModule",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Register(Registered),
    Nodes(Vec<(u64, Registration)>),
    Module(Option<Vec<u8>>),
    ModuleId(u64),
    Error(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registration {
    pub node_address: SocketAddr,
    pub node_name: String,
    pub signing_request: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registered {
    pub node_id: u64,
    pub signed_cert: String,
}
