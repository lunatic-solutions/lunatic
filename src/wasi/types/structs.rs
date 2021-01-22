use core::slice;
use std::io::{IoSlice, IoSliceMut};

use smallvec::SmallVec;
use uptown_funk::types::CReprWasmType;

use super::{aliases::*, Status};
#[derive(Copy, Clone)]
#[repr(C)]
pub struct Dirent {
    /// The offset of the next directory entry stored in this directory.
    pub d_next: Dircookie,
    /// The serial number of the file referred to by this directory entry.
    pub d_ino: Inode,
    /// The length of the name of the directory entry.
    pub d_namlen: Dirnamlen,
    /// The type of the file referred to by this directory entry.
    pub d_type: Filetype,
}

impl CReprWasmType for Dirent {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Fdstat {
    /// File type.
    pub fs_filetype: Filetype,
    /// File descriptor flags.
    pub fs_flags: Fdflags,
    /// Rights that apply to this file descriptor.
    pub fs_rights_base: Rights,
    /// Maximum set of rights that may be installed on new file descriptors that
    /// are created through this file descriptor, e.g., through `path_open`.
    pub fs_rights_inheriting: Rights,
}

impl CReprWasmType for Fdstat {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Filestat {
    /// Device ID of device containing the file.
    pub dev: Device,
    /// File serial number.
    pub ino: Inode,
    /// File type.
    pub filetype: Filetype,
    /// Number of hard links to the file.
    pub nlink: Linkcount,
    /// For regular files, the file size in bytes. For symbolic links, the length in bytes of the pathname contained in the symbolic link.
    pub size: Filesize,
    /// Last data access timestamp.
    pub atim: Timestamp,
    /// Last data modification timestamp.
    pub mtim: Timestamp,
    /// Last file status change timestamp.
    pub ctim: Timestamp,
}

impl CReprWasmType for Filestat {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct EventFdReadwrite {
    /// The number of bytes available for reading or writing.
    pub nbytes: Filesize,
    /// The state of the file descriptor.
    pub flags: Eventrwflags,
}

impl CReprWasmType for EventFdReadwrite {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Event {
    /// User-provided value that got attached to `subscription::userdata`.
    pub userdata: Userdata,
    /// If non-zero, an error that occurred while processing the subscription request.
    pub error: Status,
    /// The type of event that occured
    pub r#type: Eventtype,
    /// The contents of the event, if it is an `eventtype::fd_read` or
    /// `eventtype::fd_write`. `eventtype::clock` events ignore this field.
    pub fd_readwrite: EventFdReadwrite,
}

impl CReprWasmType for Event {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct SubscriptionClock {
    /// The clock against which to compare the timestamp.
    pub id: Clockid,
    /// The absolute or relative timestamp.
    pub timeout: Timestamp,
    /// The amount of time that the implementation may wait additionally
    /// to coalesce with other events.
    pub precision: Timestamp,
    /// Flags specifying whether the timeout is absolute or relative
    pub flags: Subclockflags,
}

impl CReprWasmType for SubscriptionClock {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct SubscriptionFdReadwrite {
    /// The file descriptor on which to wait for it to become ready for reading or writing.
    pub file_descriptor: Fd,
}

impl CReprWasmType for SubscriptionFdReadwrite {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct SubscriptionU {
    pub tag: Eventtype,
    pub u: SubscriptionUU,
}

impl CReprWasmType for SubscriptionU {}

#[derive(Copy, Clone)]
#[repr(C)]
pub union SubscriptionUU {
    pub clock: SubscriptionClock,
    pub fd_read: SubscriptionFdReadwrite,
    pub fd_write: SubscriptionFdReadwrite,
}

impl CReprWasmType for SubscriptionUU {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Subscription {
    /// User-provided value that is attached to the subscription in the
    /// implementation and returned through `event::userdata`.
    pub userdata: Userdata,
    /// The type of the event to which to subscribe, and its contents
    pub u: SubscriptionU,
}

impl CReprWasmType for Subscription {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Prestat {
    pr_type: u8,
    u: PrestatU,
}

impl CReprWasmType for Prestat {}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PrestatDir {
    /// The length of the directory name for use with `fd_prestat_dir_name`.
    pub pr_name_len: Size,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union PrestatU {
    pub dir: PrestatDir,
}

impl Prestat {
    pub fn directory(len: u32) -> Prestat {
        Prestat {
            pr_type: 0,
            u: PrestatU {
                dir: PrestatDir { pr_name_len: len },
            },
        }
    }
}

#[repr(C)]
pub struct _wasi_iovec_t {
    pub buf: u32,
    pub buf_len: u32,
}
/// A region of memory used as SOURCE for gather WRITES.
pub struct WasiConstIoVec<'a> {
    slice: &'a [u8],
}

impl<'a> WasiConstIoVec<'a> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize) -> Self {
        unsafe {
            let wasi_iovec = memory.add(ptr) as *const _wasi_iovec_t;
            let slice_ptr = memory.add((*wasi_iovec).buf as usize);
            let slice_len = (*wasi_iovec).buf_len as usize;
            let slice = slice::from_raw_parts(slice_ptr, slice_len);
            Self { slice }
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.slice
    }
}

impl<'a> From<WasiConstIoVec<'a>> for IoSlice<'a> {
    fn from(wasi_io_vec: WasiConstIoVec) -> IoSlice {
        IoSlice::new(wasi_io_vec.slice)
    }
}

/// A region of memory used as DESTINATION for scatter READS.
pub struct WasiIoVec<'a> {
    slice: &'a mut [u8],
}

impl<'a> WasiIoVec<'a> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize) -> Self {
        unsafe {
            let wasi_iovec = memory.add(ptr) as *mut _wasi_iovec_t;
            let slice_ptr = memory.add((*wasi_iovec).buf as usize);
            let slice_len = (*wasi_iovec).buf_len as usize;
            let slice = slice::from_raw_parts_mut(slice_ptr, slice_len);
            Self { slice }
        }
    }

    #[inline(always)]
    pub fn from_wasi_iovec_t(memory: *mut u8, buf: usize, buf_len: usize) -> Self {
        unsafe {
            let slice_ptr = memory.add(buf);
            let slice = slice::from_raw_parts_mut(slice_ptr, buf_len);
            Self { slice }
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.slice
    }
}

impl<'a> From<WasiIoVec<'a>> for IoSliceMut<'a> {
    fn from(wasi_io_vec: WasiIoVec) -> IoSliceMut {
        IoSliceMut::new(wasi_io_vec.slice)
    }
}

/// Array of WasiConstIoVecs, internally represented as IoSlices
pub struct WasiConstIoVecArray<'a> {
    io_slices: SmallVec<[IoSlice<'a>; 4]>,
}

impl<'a> WasiConstIoVecArray<'a> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize, len: usize) -> Self {
        let mut io_slices = SmallVec::with_capacity(len);
        for i in 0..len {
            let ptr = ptr + i * std::mem::size_of::<_wasi_iovec_t>();
            let wasi_io_vec = WasiConstIoVec::from(memory, ptr);
            io_slices.push(wasi_io_vec.into());
        }
        Self { io_slices }
    }

    pub fn get_io_slices(&self) -> &[IoSlice<'a>] {
        self.io_slices.as_slice()
    }
}

/// Array of WasiIoVecs, internally represented as IoSliceMuts
pub struct WasiIoVecArray<'a> {
    io_slices: Vec<IoSliceMut<'a>>,
}

impl<'a> WasiIoVecArray<'a> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize, len: usize) -> Self {
        let mut io_slices = Vec::with_capacity(len);
        for i in 0..len {
            let ptr = ptr + i * std::mem::size_of::<_wasi_iovec_t>();
            let wasi_io_vec = WasiIoVec::from(memory, ptr);
            io_slices.push(wasi_io_vec.into());
        }
        Self { io_slices }
    }

    pub fn get_io_slices_mut(&mut self) -> &mut [IoSliceMut<'a>] {
        self.io_slices.as_mut_slice()
    }
}
