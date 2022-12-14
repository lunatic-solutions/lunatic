use wasmtime::Trap;

use super::{constants, SqliteRow, SqliteValue};

pub type EncodeResult = Result<EncodeStream, Trap>;

pub trait Encode: Sized {
    fn encode(&self, output: EncodeStream) -> EncodeResult;
}

#[derive(Debug, Default)]
pub struct EncodeStream(pub(crate) Vec<u8>);

impl EncodeStream {
    fn write_bytes(&mut self, buf: &[u8]) {
        self.0.extend_from_slice(buf);
    }

    fn write_single(&mut self, b: u8) {
        self.0.push(b);
    }
}

impl Encode for SqliteValue {
    fn encode(&self, mut output: EncodeStream) -> EncodeResult {
        match self {
            SqliteValue::Blob(v) => {
                output.write_single(constants::SQL_KIND_BLOB);
                output.write_bytes(&(v.len() as u32).to_le_bytes());
                output.write_bytes(&v);
            }
            SqliteValue::Null => output.write_single(constants::SQL_KIND_NULL),
            SqliteValue::Text(t) => {
                output.write_single(constants::SQL_KIND_TEXT);
                output.write_bytes(&(t.len() as u32).to_le_bytes());
                output.write_bytes(t.as_bytes());
            }
            SqliteValue::Double(d) => {
                output.write_single(constants::SQL_KIND_DOUBLE);
                output.write_bytes(&d.to_le_bytes());
            }
            SqliteValue::Integer(i) => {
                output.write_single(constants::SQL_KIND_INT);
                output.write_bytes(&(*i as i32).to_le_bytes());
            }
            SqliteValue::I64(i) => {
                output.write_single(constants::SQL_KIND_INT64);
                output.write_bytes(&i.to_le_bytes());
            }
        }
        Ok(output)
    }
}

impl Encode for SqliteRow {
    fn encode(&self, mut output: EncodeStream) -> EncodeResult {
        // first, write the length of columns
        output.write_bytes(&(self.0.len() as u32).to_le_bytes());
        for value in self.0.iter() {
            output = value.encode(output)?;
        }
        Ok(output)
    }
}

pub fn encode_value<E: Encode>(data: &E) -> EncodeResult {
    let output = EncodeStream::default();
    data.encode(output)
}
