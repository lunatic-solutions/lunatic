#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use lunatic_common_api::IntoTrap;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use sqlite::Statement;

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

#[cfg(target_arch = "wasm32")]
impl SqliteRow {
    pub fn get_column(&self, idx: i32) -> Option<&SqliteValue> {
        self.0.get(idx as usize)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SqliteRow {
    pub fn read_row(statement: &mut Statement) -> Result<SqliteRow> {
        let mut row = SqliteRow::default();
        for column_idx in 0..statement.column_count() {
            row.0.push(SqliteValue::read_column(statement, column_idx)?);
        }
        Ok(row)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<'stmt> SqliteValue {
    pub fn read_column(statement: &'stmt Statement, col_idx: usize) -> Result<SqliteValue> {
        match statement.column_type(col_idx).or_trap("read_column")? {
            sqlite::Type::Binary => {
                let bytes = statement
                    .read::<Vec<u8>, usize>(col_idx)
                    .or_trap("lunatic::sqlite::query_prepare::read_binary")?;

                Ok(SqliteValue::Blob(bytes))
            }
            sqlite::Type::Float => Ok(SqliteValue::Double(
                statement
                    .read::<f64, usize>(col_idx)
                    .or_trap("lunatic::sqlite::query_prepare::read_float")?,
            )),
            sqlite::Type::Integer => Ok(SqliteValue::Integer(
                statement
                    .read::<i64, usize>(col_idx)
                    .or_trap("lunatic::sqlite::query_prepare::read_integer")?,
            )),
            sqlite::Type::String => {
                let bytes = statement
                    .read::<String, usize>(col_idx)
                    .or_trap("lunatic::sqlite::query_prepare::read_string")?;

                Ok(SqliteValue::Text(bytes))
            }
            sqlite::Type::Null => Ok(SqliteValue::Null),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl SqliteValue {
    pub fn read_text(&self) -> &str {
        if let SqliteValue::Text(text) = self {
            return text.as_str();
        }
        panic!("Trying to read non-text value as text");
    }

    pub fn read_text_string(&self) -> String {
        if let SqliteValue::Text(text) = self {
            return text.clone();
        }
        panic!("Trying to read non-text value as text");
    }

    pub fn read_blob(&self) -> &[u8] {
        if let SqliteValue::Blob(blob) = self {
            return blob.as_slice();
        }
        panic!("Trying to read non-blob value as blob");
    }

    pub fn read_integer(&self) -> i32 {
        if let SqliteValue::Integer(int) = self {
            return *int as i32;
        }
        panic!("Trying to read non-integer value as integer");
    }

    pub fn read_long(&self) -> i64 {
        if let SqliteValue::I64(int) = self {
            return *int;
        }
        panic!("Trying to read non-long value as long");
    }

    pub fn read_double(&self) -> f64 {
        if let SqliteValue::Double(double) = self {
            return *double;
        }
        panic!("Trying to read non-double value as double");
    }
}
