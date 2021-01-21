#![allow(dead_code)]

mod aliases;
mod status;

pub use aliases::*;
pub use status::Status;

use std::mem::size_of;
use std::slice::{from_raw_parts, from_raw_parts_mut};
use std::str;
use std::{
    io::{IoSlice, IoSliceMut},
    marker::PhantomData,
};

use smallvec::SmallVec;
use uptown_funk::{types, Executor, FromWasm, StateMarker, Trap};
// TODO remove dependency
use wasi_common::wasi::types::{Clockid, Fd};

pub struct MyFd {}
impl StateMarker for MyFd {}

impl FromWasm for MyFd {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized,
    {
        Ok(MyFd {})
    }
}

pub struct Wrap<T> {
    pub inner: T,
}

impl<T> Wrap<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl FromWasm for Wrap<Clockid> {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized,
    {
        use std::convert::TryFrom;
        Clockid::try_from(from)
            .map_err(|_| Trap::new("Invalid clock id"))
            .map(|v| Wrap::new(v))
    }
}

impl FromWasm for Wrap<Fd> {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized,
    {
        Ok(Wrap::new(Fd::from(from)))
    }
}

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

pub struct WasiEnv {
    bytes: Vec<Vec<u8>>,
    total_bytes: u32,
}

impl WasiEnv {
    pub fn env_vars(vars: impl Iterator<Item = (String, String)>) -> Self {
        let mut bytes = vec![];
        for (k, v) in vars {
            bytes.push(format!("{}={}\0", k, v).into_bytes());
        }

        let total_bytes = bytes.iter().map(|v| v.len() as u32).sum();

        Self { bytes, total_bytes }
    }

    pub fn args(vars: impl Iterator<Item = String>) -> Self {
        let mut bytes = vec![];
        for v in vars {
            bytes.push(format!("{}\0", v).into_bytes());
        }

        let total_bytes = bytes.iter().map(|v| v.len() as u32).sum();

        Self { bytes, total_bytes }
    }

    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub fn total_bytes(&self) -> u32 {
        self.total_bytes
    }

    pub fn iter(&self) -> std::slice::Iter<Vec<u8>> {
        self.bytes.iter()
    }
}

pub struct ExitCode<S> {
    _stats: PhantomData<S>,
}

impl<S: StateMarker> FromWasm for ExitCode<S> {
    type From = u32;
    type State = S;

    fn from(
        _: &mut Self::State,
        _: &impl Executor,
        exit_code: u32,
    ) -> Result<Self, uptown_funk::Trap> {
        Err(uptown_funk::Trap::new(format!(
            "proc_exit({}) called",
            exit_code
        )))
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Prestat {
    pr_type: u8,
    u: PrestatU,
}

impl Prestat {
    pub fn directory(len: u32) -> Prestat {
        Prestat {
            pr_type: 0,
            u: PrestatU {
                dir: PrestatUDir { pr_name_len: len },
            },
        }
    }
}

impl types::CReprWasmType for Prestat {}

#[derive(Copy, Clone)]
#[repr(C)]
pub union PrestatU {
    dir: PrestatUDir,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PrestatUDir {
    pr_name_len: u32,
}

pub const WASI_ESUCCESS: u32 = 0;
pub const WASI_E2BIG: u32 = 1;
pub const WASI_EACCES: u32 = 2;
pub const WASI_EADDRINUSE: u32 = 3;
pub const WASI_EADDRNOTAVAIL: u32 = 4;
pub const WASI_EAFNOSUPPORT: u32 = 5;
pub const WASI_EAGAIN: u32 = 6;
pub const WASI_EALREADY: u32 = 7;
pub const WASI_EBADF: u32 = 8;
pub const WASI_EBADMSG: u32 = 9;
pub const WASI_EBUSY: u32 = 10;
pub const WASI_ECANCELED: u32 = 11;
pub const WASI_ECHILD: u32 = 12;
pub const WASI_ECONNABORTED: u32 = 13;
pub const WASI_ECONNREFUSED: u32 = 14;
pub const WASI_ECONNRESET: u32 = 15;
pub const WASI_EDEADLK: u32 = 16;
pub const WASI_EDESTADDRREQ: u32 = 17;
pub const WASI_EDOM: u32 = 18;
pub const WASI_EDQUOT: u32 = 19;
pub const WASI_EEXIST: u32 = 20;
pub const WASI_EFAULT: u32 = 21;
pub const WASI_EFBIG: u32 = 22;
pub const WASI_EHOSTUNREACH: u32 = 23;
pub const WASI_EIDRM: u32 = 24;
pub const WASI_EILSEQ: u32 = 25;
pub const WASI_EINPROGRESS: u32 = 26;
pub const WASI_EINTR: u32 = 27;
pub const WASI_EINVAL: u32 = 28;
pub const WASI_EIO: u32 = 29;
pub const WASI_EISCONN: u32 = 30;
pub const WASI_EISDIR: u32 = 31;
pub const WASI_ELOOP: u32 = 32;
pub const WASI_EMFILE: u32 = 33;
pub const WASI_EMLINK: u32 = 34;
pub const WASI_EMSGSIZE: u32 = 35;
pub const WASI_EMULTIHOP: u32 = 36;
pub const WASI_ENAMETOOLONG: u32 = 37;
pub const WASI_ENETDOWN: u32 = 38;
pub const WASI_ENETRESET: u32 = 39;
pub const WASI_ENETUNREACH: u32 = 40;
pub const WASI_ENFILE: u32 = 41;
pub const WASI_ENOBUFS: u32 = 42;
pub const WASI_ENODEV: u32 = 43;
pub const WASI_ENOENT: u32 = 44;
pub const WASI_ENOEXEC: u32 = 45;
pub const WASI_ENOLCK: u32 = 46;
pub const WASI_ENOLINK: u32 = 47;
pub const WASI_ENOMEM: u32 = 48;
pub const WASI_ENOMSG: u32 = 49;
pub const WASI_ENOPROTOOPT: u32 = 50;
pub const WASI_ENOSPC: u32 = 51;
pub const WASI_ENOSYS: u32 = 52;
pub const WASI_ENOTCONN: u32 = 53;
pub const WASI_ENOTDIR: u32 = 54;
pub const WASI_ENOTEMPTY: u32 = 55;
pub const WASI_ENOTRECOVERABLE: u32 = 56;
pub const WASI_ENOTSOCK: u32 = 57;
pub const WASI_ENOTSUP: u32 = 58;
pub const WASI_ENOTTY: u32 = 59;
pub const WASI_ENXIO: u32 = 60;
pub const WASI_EOVERFLOW: u32 = 61;
pub const WASI_EOWNERDEAD: u32 = 62;
pub const WASI_EPERM: u32 = 63;
pub const WASI_EPIPE: u32 = 64;
pub const WASI_EPROTO: u32 = 65;
pub const WASI_EPROTONOSUPPORT: u32 = 66;
pub const WASI_EPROTOTYPE: u32 = 67;
pub const WASI_ERANGE: u32 = 68;
pub const WASI_EROFS: u32 = 69;
pub const WASI_ESPIPE: u32 = 70;
pub const WASI_ESRCH: u32 = 71;
pub const WASI_ESTALE: u32 = 72;
pub const WASI_ETIMEDOUT: u32 = 73;
pub const WASI_ETXTBSY: u32 = 74;
pub const WASI_EXDEV: u32 = 75;
pub const WASI_ENOTCAPABLE: u32 = 76;

pub const WASI_STDIN_FILENO: u32 = 0;
pub const WASI_STDOUT_FILENO: u32 = 1;
pub const WASI_STDERR_FILENO: u32 = 2;
