use super::aliases::Oflags;
use uptown_funk::{Executor, FromWasm, Trap};

// Create file if it does not exist.
pub const OFLAGS_CREAT: Oflags = 0x1;
/// Fail if not a directory.
pub const OFLAGS_DIRECTORY: Oflags = 0x2;
/// Fail if file already exists.
pub const OFLAGS_EXCL: Oflags = 0x4;
/// Truncate file to size 0.
pub const OFLAGS_TRUNC: Oflags = 0x8;

#[derive(Copy, Clone, Debug)]
pub struct OpenFlags(u16);

impl OpenFlags {
    pub fn create(self) -> bool {
        (self.0 & OFLAGS_CREAT) != 0
    }

    pub fn fail_if_not_directory(self) -> bool {
        (self.0 & OFLAGS_DIRECTORY) != 0
    }

    pub fn fail_if_exists(self) -> bool {
        (self.0 & OFLAGS_EXCL) != 0
    }

    pub fn truncate(self) -> bool {
        (self.0 & OFLAGS_TRUNC) != 0
    }
}

impl FromWasm for OpenFlags {
    type From = u32;
    type State = ();

    fn from(_: &mut (), _: &impl Executor, v: u32) -> Result<Self, Trap> {
        Ok(OpenFlags(v as u16))
    }
}
