use super::{constants, SqliteRow, SqliteValue};

#[derive(Debug, Clone)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    pub fn new<M: Into<String>>(message: M) -> Self {
        ParseError {
            message: message.into(),
        }
    }
}

type ParseResult<T> = Result<T, ParseError>;

impl Parse for SqliteValue {
    fn parse(input: &mut ParseStream<'_>) -> ParseResult<Self> {
        let byte = input.read_byte()?;
        println!("parse value -> GOT NEXT BYTE {}", byte);
        return match byte {
            constants::SQL_KIND_NULL => Ok(SqliteValue::Null),
            constants::SQL_KIND_DOUBLE | constants::SQL_KIND_INT64 => {
                let bytes = input.read(8)?;
                if byte == constants::SQL_KIND_DOUBLE {
                    return Ok(SqliteValue::Double(f64::from_le_bytes(
                        bytes.try_into().unwrap(),
                    )));
                }
                return Ok(SqliteValue::I64(i64::from_le_bytes(
                    bytes.try_into().unwrap(),
                )));
            }
            constants::SQL_KIND_INT => {
                let bytes = input.read(4)?;
                return Ok(SqliteValue::Integer(
                    i32::from_le_bytes(bytes.try_into().unwrap()) as i64,
                ));
            }
            constants::SQL_KIND_BLOB | constants::SQL_KIND_TEXT => {
                // 4 bytes LE encoded for length of blob
                let len_bytes = input.read(4)?;
                // the call to try_into() will always return `Ok` because it has to be 4 bytes long
                let len = i32::from_le_bytes(len_bytes.try_into().unwrap());
                let data = input.read(len as usize)?;
                if byte == constants::SQL_KIND_BLOB {
                    return Ok(SqliteValue::Blob(data.to_vec()));
                }
                return match String::from_utf8(data.to_vec()) {
                    Ok(text) => Ok(SqliteValue::Text(text)),
                    Err(e) => Err(ParseError::new(format!(
                        "Failed to parse slice as UTF8 {:?}",
                        e
                    ))),
                };
            }
            _ => Err(ParseError::new(format!(
                "Invalid type value for BindValue {:?}",
                byte
            ))),
        };
    }
}

impl Parse for SqliteRow {
    fn parse(input: &mut ParseStream<'_>) -> ParseResult<Self> {
        // read the first 4 bytes to get the length of columns
        let val_len = input.read_u32()?;
        // try to parse a BindValue for val_len items
        println!("GOING TO READ {} columns", val_len);
        let mut items = SqliteRow::default();
        for _ in 0..val_len {
            items.0.push(SqliteValue::parse(input)?);
        }
        Ok(items)
    }
}

// ===================
// parsing tools
// ===================

pub trait Parse: Sized {
    fn parse(input: &mut ParseStream<'_>) -> ParseResult<Self>;
}

pub struct ParseStream<'a> {
    buf: &'a [u8],
    cursor: usize,
}

impl<'a> ParseStream<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, cursor: 0 }
    }

    /// get value of next `count` bytes without forwarding the cursor
    pub fn peek(&self, count: usize) -> Option<&[u8]> {
        // if there's less than `count` bytes left, return None
        if self.cursor + count >= self.buf.len() {
            return None;
        }
        Some(&self.buf[self.cursor..self.cursor + count])
    }

    /// get value of next `count` bytes AND forward the cursor
    pub fn read(&mut self, count: usize) -> ParseResult<&[u8]> {
        if self.cursor + count > self.buf.len() {
            println!(
                "TRYING TO READ {} bytes. self.cursor = {} | buf.len() = {}",
                count,
                self.cursor,
                self.buf.len()
            );
            return Err(ParseError::new(format!("Failed to read {} bytes", count)));
        }
        let cursor = self.cursor;
        self.cursor += count;
        Ok(&self.buf[cursor..cursor + count])
    }

    pub fn read_byte(&mut self) -> ParseResult<u8> {
        match self.peek_byte() {
            Some(buf) => {
                self.cursor += 1;
                Ok(buf)
            }
            None => Err(ParseError::new("failed to ready single byte")),
        }
    }

    /// get value of next 4 bytes as u32
    pub fn read_u32(&mut self) -> ParseResult<u32> {
        self.read(4).and_then(|bytes| {
            if let Ok(enc) = bytes.try_into() {
                return Ok(u32::from_le_bytes(enc));
            }
            Err(ParseError::new("Failed to read u32"))
        })
    }

    pub fn peek_byte(&self) -> Option<u8> {
        self.peek(1).map(|l| l[0])
    }

    /// get value of next `count` bytes without forwarding the cursor
    pub fn skip(&self, count: usize) -> Option<&[u8]> {
        if self.cursor >= self.buf.len() {
            return None;
        }
        if self.buf.len() <= self.cursor + count {
            return Some(&self.buf[self.cursor..]);
        }
        Some(&self.buf[self.cursor..self.cursor + count])
    }
}
