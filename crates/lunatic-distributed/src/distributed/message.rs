use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Spawn(Spawn),
    Message {
        environment_id: u64,
        process_id: u64,
        tag: Option<i64>,
        data: Vec<u8>,
    },
}

impl Request {
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Spawn(_) => "Spawn",
            Request::Message { .. } => "Message",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spawn {
    pub environment_id: u64,
    pub module_id: u64,
    pub function: String,
    pub params: Vec<Val>,
    pub config: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientError {
    Unexpected(String),
    Connection(String),
    NodeNotFound,
    ModuleNotFound,
    ProcessNotFound,
}

impl Default for ClientError {
    fn default() -> Self {
        Self::Unexpected("Unexpected error.".to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Spawned(u64),
    Sent,
    Linked,
    Error(ClientError),
}

impl Response {
    pub fn kind(&self) -> &'static str {
        match self {
            Response::Spawned(_) => "Spawned",
            Response::Sent => "Sent",
            Response::Linked => "Linked",
            Response::Error(_) => "Error",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Val {
    I32(i32),
    I64(i64),
    V128(u128),
}

#[allow(clippy::from_over_into)]
impl Into<wasmtime::Val> for Val {
    fn into(self) -> wasmtime::Val {
        match self {
            Val::I32(v) => wasmtime::Val::I32(v),
            Val::I64(v) => wasmtime::Val::I64(v),
            Val::V128(v) => wasmtime::Val::V128(v),
        }
    }
}

pub fn pack_response(msg_id: u64, resp: Response) -> [Bytes; 2] {
    let data = rmp_serde::to_vec(&(msg_id, resp)).unwrap();
    let size = (data.len() as u32).to_le_bytes();
    let size: Bytes = Bytes::copy_from_slice(&size[..]);
    let bytes: Bytes = data.into();
    [size, bytes]
}
