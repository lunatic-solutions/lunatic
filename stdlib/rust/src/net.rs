use crate::{drop, Externref};

pub mod stdlib {
    use crate::Externref;

    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn tcp_bind_str(addr_ptr: *const u8, addr_len: usize, listener: *mut Externref) -> i32;
        pub fn tcp_accept(
            listener: Externref,
            tcp_socket: *mut Externref,
            addr: *mut Externref,
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
