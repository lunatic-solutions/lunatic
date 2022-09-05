use std::{collections::HashMap, net::SocketAddr};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Register(Registration),
    // Currently a node will send it's own id. We need to refactor this part: the control server
    // should always handle registration first and later know which node is sending requests.
    Deregister(u64),
    ListNodes,
    LookupNodes(String),
    AddModule(Vec<u8>),
    GetModule(u64),
}

impl Request {
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Register(_) => "Register",
            Request::Deregister(_) => "Deregister",
            Request::ListNodes => "ListNodes",
            Request::LookupNodes(_) => "LookupNodes",
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
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registration {
    pub node_address: SocketAddr,
    pub node_name: String,
    pub signing_request: String,
    pub node_metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registered {
    pub node_id: u64,
    pub signed_cert: String,
}
