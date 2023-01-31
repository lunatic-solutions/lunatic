use std::ops::Deref;

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod host_api;

/// Struct used for binding a certain `BindValue` to either
/// a numeric key or a named key in a prepared statement
#[derive(Debug, Serialize, Deserialize)]
pub enum BindKey {
    /// Is encoded as 0x00
    None,
    /// Is encoded as 0x01
    /// and uses a u32 for length of stream
    /// which is stored as usize because the sqlite library needs usize
    /// and this will save repeated conversions
    Numeric(usize),
    /// Is encoded as 0x02
    /// indicates that a string is used as index for the bind value
    String(String),
}

/// Represents a pair of BindKey and BindValue
/// that are used to bind certain data to a prepared statement
/// where BindKey is usually either a numeric index
/// starting with 1 or a string.
#[derive(Debug, Serialize, Deserialize)]
pub struct BindPair(pub BindKey, pub BindValue);

/// Enum that represents possible different
/// types of values that can be bound to a prepared statements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BindValue {
    Null,
    Blob(Vec<u8>),
    Text(String),
    Double(f64),
    Int(i32),
    Int64(i64),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BindList(pub Vec<BindPair>);

impl Deref for BindList {
    type Target = Vec<BindPair>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// ============================
// Error structure
// ============================
/// Error structure that carries data from sqlite_errmsg and sqlite_errcode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SqliteError {
    pub code: Option<u32>,
    pub message: Option<String>,
}
