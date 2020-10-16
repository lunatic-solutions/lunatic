// wasi_snapshot_preview1 types

#![allow(dead_code)]

use std::io::{Error, Write};
use std::iter::Iterator;
use std::marker::PhantomData;
use std::mem::size_of;
use std::slice::from_raw_parts_mut;

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
pub struct __wasi_iovec_t {
    pub buf: u32,
    pub buf_len: u32,
}
/// A read/write WASI iovec type. Represents both a iovec and ciovec.
pub struct WasiIoVec<'a> {
    slice: &'a mut [u8],
}

impl<'a> WasiIoVec<'a> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize) -> Self {
        unsafe {
            let wasi_iovec = memory.add(ptr) as *mut __wasi_iovec_t;
            let slice_ptr = memory.add((*wasi_iovec).buf as usize);
            let slice_len = (*wasi_iovec).buf_len as usize;
            let slice = from_raw_parts_mut(slice_ptr, slice_len);
            Self { slice }
        }
    }

    #[inline(always)]
    pub fn write<W: Write>(&self, dest: &mut W) -> Result<usize, Error> {
        dest.write(self.slice)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.slice
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.slice
    }
}

/// Iterator over the WASI (c)iovec_array type.
pub struct WasiIoVecArrayIter<'a> {
    memory: *mut u8,
    ptr: usize,
    current: usize,
    len: usize,
    phantom: PhantomData<&'a ()>,
}

impl WasiIoVecArrayIter<'_> {
    #[inline(always)]
    pub fn from(memory: *mut u8, ptr: usize, len: usize) -> Self {
        Self {
            memory,
            ptr: ptr,
            current: 0,
            len: len as usize,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    pub fn write<W: Write>(self, dest: &mut W) -> Result<usize, Error> {
        let mut bytes_written = 0;
        for io_vec in self {
            bytes_written += io_vec.write(dest)?;
        }
        Ok(bytes_written)
    }
}

impl<'a> Iterator for WasiIoVecArrayIter<'a> {
    type Item = WasiIoVec<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.len {
            let wasm_iovec = WasiIoVec::from(self.memory, self.ptr);
            self.current += 1;
            self.ptr += size_of::<__wasi_iovec_t>();
            Some(wasm_iovec)
        } else {
            None
        }
    }
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
