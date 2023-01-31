mod bind_values;
mod sqlite_value;

pub use bind_values::*;
pub use sqlite_value::*;

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum DbError<'a> {
    /// contains path to which access was attempted
    PermissionDenied(&'a str),
}
