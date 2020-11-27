use crate::{clone, drop, Externref};

use std::fmt;
use std::io::{self, Error, ErrorKind, IoSlice, IoSliceMut, Write, Read};
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

pub mod stdlib {
    use crate::Externref;
    use std::io::{IoSlice, IoSliceMut};

    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn tcp_bind_str(addr_ptr: *const u8, addr_len: usize, listener: *mut Externref) -> i32;
        pub fn tcp_accept(
            listener: Externref,
            tcp_socket: *mut Externref,
            addr: *mut Externref,
        ) -> i32;
        pub fn tcp_stream_serialize(tcp_stream: Externref) -> u64;
        pub fn tcp_stream_deserialize(tcp_stream: u64) -> Externref;
    }

    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    extern "C" {
        pub fn fd_write(
            tcp_stream: Externref,
            ciovs_ptr: *const IoSlice<'_>,
            ciovs_len: usize,
            nwritten_ptr: *mut usize,
        ) -> i32;

        pub fn fd_read(
            tcp_stream: Externref,
            iovs_ptr: *mut IoSliceMut<'_>,
            iovs_len: usize,
            nread_ptr: *mut usize,
        ) -> i32;
    }
}

pub struct TcpListener {
    externref: Externref,
}

impl TcpListener {
    pub fn bind(addr: &str) -> Result<Self, i32> {
        let mut externref = 0;
        let result = unsafe {
            stdlib::tcp_bind_str(addr.as_ptr(), addr.len(), &mut externref as *mut Externref)
        };
        if result == 0 {
            Ok(Self { externref })
        } else {
            Err(result)
        }
    }

    pub fn accept(&self) -> Result<TcpStream, i32> {
        let mut tcp_stream_externref = 0;
        let mut socket_addr_externref = 0;
        let result = unsafe {
            stdlib::tcp_accept(
                self.externref,
                &mut tcp_stream_externref as *mut Externref,
                &mut socket_addr_externref as *mut Externref,
            )
        };
        if result == 0 {
            // TODO: We never use socket_addr_externref, this leaks the externref.
            Ok(TcpStream {
                externref: tcp_stream_externref,
            })
        } else {
            Err(result)
        }
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        drop(self.externref);
    }
}

pub struct TcpStream {
    externref: Externref,
}

impl TcpStream {
    pub unsafe fn from_externref(externref: Externref) -> Self {
        Self {
            externref
        }
    }
}

impl Clone for TcpStream {
    fn clone(&self) -> Self {
        let externref = clone(self.externref);
        Self {
            externref
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        drop(self.externref);
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let io_slice = IoSlice::new(buf);
        self.write_vectored(&[io_slice])
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut nwritten: usize = 0;
        let result = unsafe {
            stdlib::fd_write(
                self.externref,
                bufs.as_ptr(),
                bufs.len(),
                &mut nwritten as *mut usize,
            )
        };
        if result == 0 {
            Ok(nwritten)
        } else {
            Err(Error::new(
                ErrorKind::Other,
                format!("write_vectored error: {}", result),
            ))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let io_slice = IoSliceMut::new(buf);
        self.read_vectored(&mut [io_slice])
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let mut nread: usize = 0;
        let result = unsafe {
            stdlib::fd_read(
                self.externref,
                bufs.as_mut_ptr(),
                bufs.len(),
                &mut nread as *mut usize,
            )
        };
        if result == 0 {
            Ok(nread)
        } else {
            Err(Error::new(
                ErrorKind::Other,
                format!("read_vectored error: {}", result),
            ))
        }
    }
}


impl Serialize for TcpStream {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let serialized_tcp_stream = unsafe { stdlib::tcp_stream_serialize(self.externref) };
        serializer.serialize_u64(serialized_tcp_stream)
    }
}

struct TcpStreamVisitor {}

impl<'de> Visitor<'de> for  TcpStreamVisitor {
    type Value = TcpStream;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an pointer to an externref containing a  tcp_stream")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let  tcp_stream_externref = unsafe { stdlib::tcp_stream_deserialize(value) };
        unsafe { Ok(TcpStream::from_externref( tcp_stream_externref)) }
    }
}

impl<'de> Deserialize<'de> for TcpStream {
    fn deserialize<D>(deserializer: D) -> Result<TcpStream, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_u64( TcpStreamVisitor {})
    }
}