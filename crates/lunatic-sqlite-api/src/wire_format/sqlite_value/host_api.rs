#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use lunatic_common_api::IntoTrap;

#[cfg(not(target_arch = "wasm32"))]
use sqlite::Statement;

use super::{SqliteRow, SqliteValue};

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
