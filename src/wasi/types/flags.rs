use std::io::SeekFrom;

use super::{aliases::Oflags, Filedelta};
use uptown_funk::{types::CReprWasmType, Executor, FromWasm, ToWasm, Trap};

// Create file if it does not exist.
pub const OFLAGS_CREAT: Oflags = 0x1;
/// Fail if not a directory.
pub const OFLAGS_DIRECTORY: Oflags = 0x2;
/// Fail if file already exists.
pub const OFLAGS_EXCL: Oflags = 0x4;
/// Truncate file to size 0.
pub const OFLAGS_TRUNC: Oflags = 0x8;

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
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

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum Whence {
    /// Seek relative to start-of-file.
    Start = 0,
    /// Seek relative to current position.
    Current = 1,
    /// Seek relative to end-of-file.
    End = 2,
}

impl Whence {
    pub fn into_seek_from(self, delta: Filedelta) -> SeekFrom {
        match self {
            Whence::Start => SeekFrom::Start(delta as u64),
            Whence::Current => SeekFrom::Current(delta),
            Whence::End => SeekFrom::End(delta),
        }
    }
}

impl CReprWasmType for Whence {}

impl ToWasm for Whence {
    type To = u32;
    type State = ();

    fn to(_: &mut (), _: &impl Executor, v: Self) -> Result<u32, Trap> {
        Ok(v as u32)
    }
}

impl FromWasm for Whence {
    type From = u32;
    type State = ();

    fn from(_: &mut (), _: &impl Executor, from: u32) -> Result<Self, Trap> {
        match from {
            0 => Ok(Whence::Start),
            1 => Ok(Whence::Current),
            2 => Ok(Whence::End),
            _ => Err(Trap::new("Invalid whence")),
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Filetype {
    /// The type of the file descriptor or file is unknown or is different from any of the other types specified.
    Unknown = 0,
    /// The file descriptor or file refers to a block device inode.
    BlockDevice = 1,
    /// The file descriptor or file refers to a character device inode.
    CharacterDevice = 2,
    /// The file descriptor or file refers to a directory inode.
    Directory = 3,
    /// The file descriptor or file refers to a regular file inode.
    RegularFile = 4,
    /// The file descriptor or file refers to a datagram socket.
    SocketDgram = 5,
    /// The file descriptor or file refers to a byte-stream socket.
    SocketStream = 6,
    /// The file refers to a symbolic link inode.
    SymbolicLink = 7,
}

impl CReprWasmType for Filetype {}

impl ToWasm for Filetype {
    type To = u32;
    type State = ();

    fn to(_: &mut (), _: &impl Executor, v: Self) -> Result<u32, Trap> {
        Ok(v as u32)
    }
}

impl FromWasm for Filetype {
    type From = u32;
    type State = ();

    fn from(_: &mut (), _: &impl Executor, from: u32) -> Result<Self, Trap> {
        match from {
            0 => Ok(Filetype::Unknown),
            1 => Ok(Filetype::BlockDevice),
            2 => Ok(Filetype::CharacterDevice),
            3 => Ok(Filetype::Directory),
            4 => Ok(Filetype::RegularFile),
            5 => Ok(Filetype::SocketDgram),
            6 => Ok(Filetype::SocketStream),
            7 => Ok(Filetype::SymbolicLink),
            _ => Err(Trap::new("Invalid file type")),
        }
    }
}

impl From<std::fs::FileType> for Filetype {
    fn from(ft: std::fs::FileType) -> Self {
        if ft.is_dir() {
            return Filetype::Directory;
        }

        if ft.is_file() {
            return Filetype::RegularFile;
        }

        if ft.is_symlink() {
            return Filetype::SymbolicLink;
        }

        return Filetype::Unknown;
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct Fdflags(u16);

/// Append mode: Data written to the file is always appended to the file's end.
pub const FDFLAGS_APPEND: u16 = 0x1;
/// Write according to synchronized I/O data integrity completion. Only the data stored in the file is synchronized.
pub const FDFLAGS_DSYNC: u16 = 0x2;
/// Non-blocking mode.
pub const FDFLAGS_NONBLOCK: u16 = 0x4;
/// Synchronized read I/O operations.
pub const FDFLAGS_RSYNC: u16 = 0x8;
/// Write according to synchronized I/O file integrity completion. In
/// addition to synchronizing the data stored in the file, the implementation
/// may also synchronously update the file's metadata.
pub const FDFLAGS_SYNC: u16 = 0x10;

impl Fdflags {
    pub fn new() -> Self {
        Fdflags(0)
    }

    pub fn is_append(self) -> bool {
        (self.0 & FDFLAGS_APPEND) != 0
    }

    pub fn is_dsync(self) -> bool {
        (self.0 & FDFLAGS_DSYNC) != 0
    }

    pub fn is_nonblock(self) -> bool {
        (self.0 & FDFLAGS_NONBLOCK) != 0
    }

    pub fn is_rsync(self) -> bool {
        (self.0 & FDFLAGS_RSYNC) != 0
    }

    pub fn is_sync(self) -> bool {
        (self.0 & FDFLAGS_SYNC) != 0
    }
}

impl FromWasm for Fdflags {
    type From = u32;
    type State = ();

    fn from(_: &mut (), _: &impl Executor, v: u32) -> Result<Self, Trap> {
        Ok(Fdflags(v as u16))
    }
}

impl ToWasm for Fdflags {
    type To = u32;
    type State = ();

    fn to(_: &mut (), _: &impl Executor, v: Self) -> Result<u32, Trap> {
        Ok(v.0 as u32)
    }
}
