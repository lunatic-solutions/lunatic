use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Register(Registration),
    // Currently a node will send it's own id. We need to refactor this part: the control server
    // should always handle registration first and later know which node is sending requests.
    Deregister(u64),
    ListNodes,
    AddModule(Vec<u8>),
    GetModule(u64),
}

impl Request {
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Register(_) => "Register",
            Request::Deregister(_) => "Deregister",
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
    None,
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

pub fn pack_response(msg_id: u64, resp: Response) -> [Bytes; 2] {
    let data = bincode::serialize(&(msg_id, resp)).unwrap();
    let size = (data.len() as u32).to_le_bytes();
    let size: Bytes = Bytes::copy_from_slice(&size[..]);
    let bytes: Bytes = data.into();
    [size, bytes]
}
