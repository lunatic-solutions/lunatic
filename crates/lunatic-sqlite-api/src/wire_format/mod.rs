use std::ops::Deref;

#[cfg(not(target_arch = "wasm32"))]
use lunatic_common_api::IntoTrap;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use sqlite::Statement;
#[cfg(not(target_arch = "wasm32"))]
use wasmtime::Trap;

mod sqlite_value;

pub use sqlite_value::*;

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

#[cfg(not(target_arch = "wasm32"))]
impl BindPair {
    pub fn bind(&self, statement: &mut Statement) -> Result<(), Trap> {
        if let BindKey::Numeric(idx) = self.0 {
            return match self.1.clone() {
                BindValue::Null => todo!(),
                BindValue::Blob(b) => statement.bind((idx, &b[..])),
                BindValue::Text(t) => statement.bind((idx, t.as_str())),
                BindValue::Double(d) => statement.bind((idx, d)),
                BindValue::Int(i) => statement.bind((idx, i as i64)),
                BindValue::Int64(i) => statement.bind((idx, i)),
            }
            .or_trap("sqlite::bind::pair");
        }
        match self.1.clone() {
            BindValue::Blob(b) => statement.bind(&[&b[..]][..]),
            BindValue::Null => todo!(),
            BindValue::Text(t) => statement.bind(&[t.as_str()][..]),
            BindValue::Double(d) => statement.bind(&[d][..]),
            BindValue::Int(i) => statement.bind(&[i as i64][..]),
            BindValue::Int64(i) => statement.bind(&[i][..]),
        }
        .or_trap("sqlite::bind::single")
    }
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

#[cfg(not(target_arch = "wasm32"))]
impl From<sqlite::Error> for SqliteError {
    fn from(err: sqlite::Error) -> Self {
        Self {
            code: err.code.map(|code| code as u32),
            message: err.message,
        }
    }
}
