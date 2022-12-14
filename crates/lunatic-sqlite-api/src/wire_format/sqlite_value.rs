#[cfg(not(target_arch = "wasm32"))]
use lunatic_common_api::IntoTrap;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use sqlite::Statement;
#[cfg(not(target_arch = "wasm32"))]
use wasmtime::Trap;

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
    pub fn read_row(statement: &mut Statement) -> Result<SqliteRow, Trap> {
        let mut row = SqliteRow::default();
        for column_idx in 0..statement.column_count() {
            row.0
                .push(SqliteValue::read_column(&statement, column_idx)?);
        }
        Ok(row)
    }
}

// use std::cell::Ref;
// use std::ptr::NonNull;
// use std::{slice, str};

// use super::row::PrivateSqliteRow;

// /// Raw sqlite value as received from the database
// ///
// /// Use existing `FromSql` implementations to convert this into
// /// rust values
// #[allow(missing_debug_implementations, missing_copy_implementations)]
// pub struct SqliteValue<'row, 'stmt, 'query> {
//     // This field exists to ensure that nobody
//     // can modify the underlying row while we are
//     // holding a reference to some row value here
//     _row: Ref<'row, PrivateSqliteRow<'stmt, 'query>>,
//     // // we extract the raw value pointer as part of the constructor
//     // // to safe the match statements for each method
//     // // According to benchmarks this leads to a ~20-30% speedup
//     // //
//     // // This is sound as long as nobody calls `stmt.step()`
//     // // while holding this value. We ensure this by including
//     // // a reference to the row above.
//     // value: NonNull<ffi::sqlite3_value>,
// }

// #[repr(transparent)]
// pub(super) struct OwnedSqliteValue {
//     pub(super) value: NonNull<ffi::sqlite3_value>,
// }

// impl Drop for OwnedSqliteValue {
//     fn drop(&mut self) {
//         unsafe { ffi::sqlite3_value_free(self.value.as_ptr()) }
//     }
// }

#[cfg(not(target_arch = "wasm32"))]
impl<'stmt> SqliteValue {
    pub fn read_column(statement: &'stmt Statement, col_idx: usize) -> Result<SqliteValue, Trap> {
        // let row = statement.
        // let value = match &*row {
        //     PrivateSqliteRow::Direct(stmt) => stmt.column_value(col_idx)?,
        //     PrivateSqliteRow::Duplicated { values, .. } => {
        //         values.get(col_idx as usize).and_then(|v| v.as_ref())?.value
        //     }
        // };

        // let ret = Self { _row: row, value };
        // if ret.value_type().is_none() {
        //     None
        // } else {
        //     Some(ret)
        // }
        match statement.column_type(col_idx).or_trap("read_column")? {
            sqlite::Type::Binary => {
                let bytes = statement
                    .read::<Vec<u8>, usize>(col_idx)
                    .or_trap("lunatic::sqlite::query_prepare::read_binary")?;

                // let len = bytes.len();
                // return_value.append(&mut vec![ColumnType::Binary as u8]);
                // return_value.append(&mut (len as u32).to_le_bytes().to_vec());
                // return_value.append(&mut bytes);
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

    // /// Get the type of the value as returned by sqlite
    // pub fn value_type(&self) -> Option<SqliteType> {
    //     let tpe = unsafe { ffi::sqlite3_value_type(self.value.as_ptr()) };
    //     match tpe {
    //         ffi::SQLITE_TEXT => Some(SqliteType::Text),
    //         ffi::SQLITE_INTEGER => Some(SqliteType::Long),
    //         ffi::SQLITE_FLOAT => Some(SqliteType::Double),
    //         ffi::SQLITE_BLOB => Some(SqliteType::Binary),
    //         ffi::SQLITE_NULL => None,
    //         _ => unreachable!(
    //             "Sqlite's documentation state that this case ({}) is not reachable. \
    //              If you ever see this error message please open an issue at \
    //              https://github.com/diesel-rs/diesel.",
    //             tpe
    //         ),
    //     }
    // }
}

#[cfg(target_arch = "wasm32")]
impl SqliteValue {
    // pub(crate) fn parse_string(&self) {
    //     let s = unsafe {
    //         let ptr = ffi::sqlite3_value_text(self.value.as_ptr());
    //         let len = ffi::sqlite3_value_bytes(self.value.as_ptr());
    //         let bytes = slice::from_raw_parts(ptr as *const u8, len as usize);
    //         // The string is guaranteed to be utf8 according to
    //         // https://www.sqlite.org/c3ref/value_blob.html
    //         str::from_utf8_unchecked(bytes)
    //     };
    //     f(s)
    // }

    pub fn read_text(&self) -> &str {
        if let SqliteValue::Text(text) = self {
            println!("READING TEXT {:?} | as_str {:?}", self, text.as_str());
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
        println!("TRYING READ INTEGER {:?}", self);
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

// impl OwnedSqliteValue {
//     pub(super) fn copy_from_ptr(ptr: NonNull<ffi::sqlite3_value>) -> Option<OwnedSqliteValue> {
//         let tpe = unsafe { ffi::sqlite3_value_type(ptr.as_ptr()) };
//         if ffi::SQLITE_NULL == tpe {
//             return None;
//         }
//         let value = unsafe { ffi::sqlite3_value_dup(ptr.as_ptr()) };
//         Some(Self {
//             value: NonNull::new(value)?,
//         })
//     }

//     pub(super) fn duplicate(&self) -> OwnedSqliteValue {
//         // self.value is a `NonNull` ptr so this cannot be null
//         let value = unsafe { ffi::sqlite3_value_dup(self.value.as_ptr()) };
//         let value = NonNull::new(value).expect(
//             "Sqlite documentation states this returns only null if value is null \
//                  or OOM. If you ever see this panic message please open an issue at \
//                  https://github.com/diesel-rs/diesel.",
//         );
//         OwnedSqliteValue { value }
//     }
// }
