use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Spawn {
        environment_id: u64,
        module_id: u64,
        function: String,
        params: Vec<Val>,
    },
    Message {
        environment_id: u64,
        process_id: u64,
        tag: Option<i64>,
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Spawned(u64),
    Linked,
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
