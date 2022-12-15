use std::ops::Deref;

#[cfg(any(unix, windows))]
use lunatic_common_api::IntoTrap;
use serde::{Deserialize, Serialize};
#[cfg(any(unix, windows))]
use sqlite::Statement;
#[cfg(any(unix, windows))]
use wasmtime::Trap;

mod sqlite_value;

pub use sqlite_value::*;

pub mod constants {
    pub const SQL_KIND_NULL: u8 = 0x00;
    pub const SQL_KIND_BLOB: u8 = 0x01;
    pub const SQL_KIND_TEXT: u8 = 0x02;
    pub const SQL_KIND_DOUBLE: u8 = 0x03;
    pub const SQL_KIND_INT: u8 = 0x04;
    pub const SQL_KIND_INT64: u8 = 0x05;
}

#[cfg(target_arch = "wasm32")]
mod parse;

#[cfg(target_arch = "wasm32")]
pub use parse::*;

#[cfg(not(target_arch = "wasm32"))]
mod encode;

#[cfg(not(target_arch = "wasm32"))]
pub use encode::*;

#[repr(u8)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BindPair(pub BindKey, pub BindValue);

// pub trait SqliteBindable {
//     fn bind(&self)
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BindValue {
    Null,
    Blob(Vec<u8>),
    Text(String),
    Double(f64),
    Int(i32),
    Int64(i64),
}

#[cfg(any(unix, windows))]
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

/// BindList is the main low-level structure that is used to encode one or more bind values
/// in the guest and the same structure is used in the host to parse the values
///
/// binary format:
/// B0: 0x01    - amount of bind values
///
/// Content:
/// B1:         - kind of sql type
/// If fixed size type read fixed bytes  - content of fixed data (not blob and not text)
/// If flexible size, read 4 bytes for size (LE encoded)
/// Read $size bytes into Vec<u8> or String
///
/// Bind key - Byte After content (BA):
/// BA0: 0x00 OR 0x01 OR 0x02             - has key or no key
/// if previous is 0x10 = No key, just use next
/// if previous is 0x20 = idx as u32
///     BA1: read 4 bytes le_encoded as u32, numeric index
/// if previous is 0x30 = &str index
///     BA2-BA5: take 4 more bytes for key $len
///     BA5->BA5+$len: read $len bytes from stream
///
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
/// and is transported in the following layout:
///
/// B0:
/// if 0x00:
///     no code available, continue with message
/// if 0x10:
///     read 4 bytes as little-endian encoded u32
///
/// Next byte after code:
/// if 0x00:
///     no message available, done
/// if 0x10:
///     read 4 bytes as little-endian encoded u32 = length of message string
///     read $len bytes and transform into String
///
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SqliteError {
    pub code: Option<u32>,
    pub message: Option<String>,
}

#[cfg(any(unix, windows))]
impl From<sqlite::Error> for SqliteError {
    fn from(err: sqlite::Error) -> Self {
        Self {
            code: err.code.map(|code| code as u32),
            message: err.message,
        }
    }
}
