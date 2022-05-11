use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Register(Registration),
    ListNodes,
    RegisterModule(Vec<u8>),
    GetModule(u64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Register(u64),
    Nodes(Vec<(u64, Registration)>),
    RegisterModule(u64),
    Module(Option<Vec<u8>>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registration {
    pub node_address: SocketAddr,
}
