use anyhow::Result;
use lunatic_common_api::IntoTrap;
use sqlite::Statement;

use super::{BindKey, BindPair, BindValue, SqliteError};

impl BindPair {
    pub fn bind(&self, statement: &mut Statement) -> Result<()> {
        if let BindKey::Numeric(idx) = self.0 {
            return match self.1.clone() {
                BindValue::Null => statement.bind((idx, ())),
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
            BindValue::Null => statement.bind(&[()][..]),
            BindValue::Text(t) => statement.bind(&[t.as_str()][..]),
            BindValue::Double(d) => statement.bind(&[d][..]),
            BindValue::Int(i) => statement.bind(&[i as i64][..]),
            BindValue::Int64(i) => statement.bind(&[i][..]),
        }
        .or_trap("sqlite::bind::single")
    }
}

// mapping of error from sqlite error
impl From<sqlite::Error> for SqliteError {
    fn from(err: sqlite::Error) -> Self {
        Self {
            code: err.code.map(|code| code as u32),
            message: err.message,
        }
    }
}
