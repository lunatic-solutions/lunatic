// wasi_snapshot_preview1 types

#![allow(dead_code)]

use std::io::{IoSlice, IoSliceMut};
use std::mem::size_of;
use std::slice::{from_raw_parts, from_raw_parts_mut};
use std::str;

use smallvec::SmallVec;

/// WASI size (u32) type
pub struct WasiSize {
    ptr: *mut u32,
}

impl WasiSize {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize) -> Self {
        Self {
            ptr: unsafe { memory.add(ptr) as *mut u32 },
        }
    }

    #[inline(always)]
    pub fn set(&mut self, value: u32) {
        unsafe {
            *(self.ptr) = value;
        }
    }

    #[inline(always)]
    pub fn get(&mut self) -> u32 {
        unsafe { *(self.ptr) }
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
            let slice = from_raw_parts(slice_ptr, slice_len);
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
            let slice = from_raw_parts_mut(slice_ptr, slice_len);
            Self { slice }
        }
    }

    #[inline(always)]
    pub fn from_wasi_iovec_t(memory: *mut u8, buf: usize, buf_len: usize) -> Self {
        unsafe {
            let slice_ptr = memory.add(buf);
            let slice = from_raw_parts_mut(slice_ptr, buf_len);
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
            let ptr = ptr + i * size_of::<_wasi_iovec_t>();
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
            let ptr = ptr + i * size_of::<_wasi_iovec_t>();
            let wasi_io_vec = WasiIoVec::from(memory, ptr);
            io_slices.push(wasi_io_vec.into());
        }
        Self { io_slices }
    }

    pub fn get_io_slices_mut(&mut self) -> &mut [IoSliceMut<'a>] {
        self.io_slices.as_mut_slice()
    }
}

#[repr(C)]
pub struct _wasi_prestat_t {
    pub union_tag: u32, // 0 for prestat_dir (only option in snapshot1)
    pub value: u32,
}

pub struct WasiPrestatDir {
    ptr: *mut _wasi_prestat_t,
}

impl WasiPrestatDir {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize) -> Self {
        Self {
            ptr: unsafe { memory.add(ptr) as *mut _wasi_prestat_t },
        }
    }

    #[inline(always)]
    pub fn set_dir_len(&mut self, dir_len: u32) {
        let prestat = _wasi_prestat_t {
            union_tag: 0,
            value: dir_len,
        };
        unsafe {
            *(self.ptr) = prestat;
        }
    }
}

pub struct WasiString {
    ptr: *mut u8,
    len: usize,
}

impl WasiString {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize, len: usize) -> Self {
        Self {
            ptr: unsafe { memory.add(ptr) as *mut u8 },
            len,
        }
    }

    #[inline(always)]
    pub fn get(&self) -> &str {
        unsafe {
            let slice = from_raw_parts(self.ptr, self.len);
            str::from_utf8(slice).unwrap()
        }
    }
}

pub const WASI_ESUCCESS: i32 = 0;
pub const WASI_E2BIG: i32 = 1;
pub const WASI_EACCES: i32 = 2;
pub const WASI_EADDRINUSE: i32 = 3;
pub const WASI_EADDRNOTAVAIL: i32 = 4;
pub const WASI_EAFNOSUPPORT: i32 = 5;
pub const WASI_EAGAIN: i32 = 6;
pub const WASI_EALREADY: i32 = 7;
pub const WASI_EBADF: i32 = 8;
pub const WASI_EBADMSG: i32 = 9;
pub const WASI_EBUSY: i32 = 10;
pub const WASI_ECANCELED: i32 = 11;
pub const WASI_ECHILD: i32 = 12;
pub const WASI_ECONNABORTED: i32 = 13;
pub const WASI_ECONNREFUSED: i32 = 14;
pub const WASI_ECONNRESET: i32 = 15;
pub const WASI_EDEADLK: i32 = 16;
pub const WASI_EDESTADDRREQ: i32 = 17;
pub const WASI_EDOM: i32 = 18;
pub const WASI_EDQUOT: i32 = 19;
pub const WASI_EEXIST: i32 = 20;
pub const WASI_EFAULT: i32 = 21;
pub const WASI_EFBIG: i32 = 22;
pub const WASI_EHOSTUNREACH: i32 = 23;
pub const WASI_EIDRM: i32 = 24;
pub const WASI_EILSEQ: i32 = 25;
pub const WASI_EINPROGRESS: i32 = 26;
pub const WASI_EINTR: i32 = 27;
pub const WASI_EINVAL: i32 = 28;
pub const WASI_EIO: i32 = 29;
pub const WASI_EISCONN: i32 = 30;
pub const WASI_EISDIR: i32 = 31;
pub const WASI_ELOOP: i32 = 32;
pub const WASI_EMFILE: i32 = 33;
pub const WASI_EMLINK: i32 = 34;
pub const WASI_EMSGSIZE: i32 = 35;
pub const WASI_EMULTIHOP: i32 = 36;
pub const WASI_ENAMETOOLONG: i32 = 37;
pub const WASI_ENETDOWN: i32 = 38;
pub const WASI_ENETRESET: i32 = 39;
pub const WASI_ENETUNREACH: i32 = 40;
pub const WASI_ENFILE: i32 = 41;
pub const WASI_ENOBUFS: i32 = 42;
pub const WASI_ENODEV: i32 = 43;
pub const WASI_ENOENT: i32 = 44;
pub const WASI_ENOEXEC: i32 = 45;
pub const WASI_ENOLCK: i32 = 46;
pub const WASI_ENOLINK: i32 = 47;
pub const WASI_ENOMEM: i32 = 48;
pub const WASI_ENOMSG: i32 = 49;
pub const WASI_ENOPROTOOPT: i32 = 50;
pub const WASI_ENOSPC: i32 = 51;
pub const WASI_ENOSYS: i32 = 52;
pub const WASI_ENOTCONN: i32 = 53;
pub const WASI_ENOTDIR: i32 = 54;
pub const WASI_ENOTEMPTY: i32 = 55;
pub const WASI_ENOTRECOVERABLE: i32 = 56;
pub const WASI_ENOTSOCK: i32 = 57;
pub const WASI_ENOTSUP: i32 = 58;
pub const WASI_ENOTTY: i32 = 59;
pub const WASI_ENXIO: i32 = 60;
pub const WASI_EOVERFLOW: i32 = 61;
pub const WASI_EOWNERDEAD: i32 = 62;
pub const WASI_EPERM: i32 = 63;
pub const WASI_EPIPE: i32 = 64;
pub const WASI_EPROTO: i32 = 65;
pub const WASI_EPROTONOSUPPORT: i32 = 66;
pub const WASI_EPROTOTYPE: i32 = 67;
pub const WASI_ERANGE: i32 = 68;
pub const WASI_EROFS: i32 = 69;
pub const WASI_ESPIPE: i32 = 70;
pub const WASI_ESRCH: i32 = 71;
pub const WASI_ESTALE: i32 = 72;
pub const WASI_ETIMEDOUT: i32 = 73;
pub const WASI_ETXTBSY: i32 = 74;
pub const WASI_EXDEV: i32 = 75;
pub const WASI_ENOTCAPABLE: i32 = 76;

pub const WASI_STDIN_FILENO: i32 = 0;
pub const WASI_STDOUT_FILENO: i32 = 1;
pub const WASI_STDERR_FILENO: i32 = 2;
