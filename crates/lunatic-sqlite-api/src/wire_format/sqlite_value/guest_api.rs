use super::{SqliteRow, SqliteValue};

#[cfg(target_arch = "wasm32")]
impl SqliteRow {
    pub fn get_column(&self, idx: i32) -> Option<&SqliteValue> {
        self.0.get(idx as usize)
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
