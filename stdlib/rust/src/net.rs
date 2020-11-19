use crate::{drop, Externref};

use std::io::{self, Error, ErrorKind, IoSlice, Write};

pub mod stdlib {
    use crate::Externref;
    use std::io::IoSlice;

    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn tcp_bind_str(addr_ptr: *const u8, addr_len: usize, listener: *mut Externref) -> i32;
        pub fn tcp_accept(
            listener: Externref,
            tcp_socket: *mut Externref,
            addr: *mut Externref,
        ) -> i32;
    }

    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    extern "C" {
        pub fn fd_write(
            tcp_stream: Externref,
            ciovs_ptr: *const IoSlice<'_>,
            ciovs_len: usize,
            nwritten_ptr: *mut usize,
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
