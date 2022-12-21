use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod host_api;
#[cfg(not(target_arch = "wasm32"))]
pub use host_api::*;

#[cfg(target_arch = "wasm32")]
mod guest_api;

#[cfg(target_arch = "wasm32")]
pub use guest_api::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SqliteValue {
    Null,
    Blob(Vec<u8>),
    Text(String),
    Double(f64),
    Integer(i64),
    I64(i64),
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SqliteRow(pub Vec<SqliteValue>);
