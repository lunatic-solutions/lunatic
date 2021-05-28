use super::{resolver::*, tcp::*};
use smol::io::{AsyncReadExt, AsyncWriteExt};
use std::io;
use std::io::{IoSlice, IoSliceMut};
use uptown_funk::host_functions;
use uptown_funk::types::Pointer as Ptr;

type OptionTrap = Result<u32, uptown_funk::Trap>;

use super::resolver::Resolver;
use super::tcp::{TcpListener, TcpStream};
use uptown_funk::state::HashMapStore;

use crate::api::channel::api::ChannelState;

pub struct TcpState {
    channel_state: ChannelState,
    pub resolvers: HashMapStore<Resolver>,
    pub listeners: HashMapStore<TcpListener>,
    pub streams: HashMapStore<TcpStream>,
}

impl TcpState {
    pub fn new(channel_state: ChannelState) -> Self {
        Self {
            channel_state,
            resolvers: HashMapStore::new(),
            listeners: HashMapStore::new(),
            streams: HashMapStore::new(),
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl TcpState {
    async fn resolve(&self, name: &str) -> (u32, ResolverResult) {
        match Resolver::resolve(name).await {
            Ok(resolver) => (0, ResolverResult::Ok(resolver)),
            Err(err) => (int_of_io_error(&err), ResolverResult::Err(())),
        }
    }

    // Result:
    // 0: Success
    // 1: No more addresses available
    fn resolve_next(
        &self,
        resolver: Resolver,
        addr: Ptr<u8>,
        mut addr_len: Ptr<u32>,
        mut port: Ptr<u16>,
        mut flowinfo: Ptr<u32>,
        mut scope_id: Ptr<u32>,
    ) -> OptionTrap {
        if let Some(address) = resolver.next() {
            match address {
                smol::net::SocketAddr::V4(v4) => {
                    let octets = v4.ip().octets();
                    addr.copy_slice(&octets)?;
                    addr_len.set(octets.len() as u32);
                }
                smol::net::SocketAddr::V6(v6) => {
                    let octets = v6.ip().octets();
                    addr.copy_slice(&octets)?;
                    addr_len.set(octets.len() as u32);
                    flowinfo.set(v6.flowinfo());
                    scope_id.set(v6.scope_id());
                }
            }
            port.set(address.port());
            Ok(0)
        } else {
            Ok(1)
        }
    }

    fn remove_resolver(&mut self, id: u32) {
        self.resolvers.remove(id);
    }

    async fn tcp_bind(&self, address: &[u8], port: u32) -> (u32, TcpListenerResult) {
        match TcpListener::bind(address, port as u16).await {
            Ok(listener) => (0, TcpListenerResult::Ok(listener)),
            Err(err) => (int_of_io_error(&e), TcpListenerResult::Err(())),
        }
    }

    async fn tcp_accept(&self, tcp_listener: TcpListener) -> (u32, TcpStreamResult) {
        match tcp_listener.accept().await {
            Ok(stream) => (0, TcpStreamResult::Ok(stream)),
            Err(err) => (int_of_io_error(&err), TcpStreamResult::Err(())),
        }
    }

    async fn tcp_connect(&self, address: &[u8], port: u32) -> (u32, TcpStreamResult) {
        match TcpStream::connect(address, port as u16).await {
            Ok(tcp_stream) => (0, TcpStreamResult::Ok(tcp_stream)),
            Err(err) => (int_of_io_error(&err), TcpStreamResult::Err(())),
        }
    }

    async fn tcp_write_vectored(
        &self,
        mut tcp_stream: TcpStream,
        ciovs: &[IoSlice<'_>],
    ) -> (u32, u32) {
        match tcp_stream.0.write_vectored(ciovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(e) => (int_of_io_error(&e), 0),
        }
    }

    async fn tcp_flush(&self, mut tcp_stream: TcpStream) -> u32 {
        match tcp_stream.0.flush().await {
            Ok(()) => 0,
            Err(e) => int_of_io_error(&e),
        }
    }

    async fn tcp_read_vectored<'a>(
        &self,
        tcp_stream: &'a mut TcpStream,
        iovs: &'a mut [IoSliceMut<'a>],
    ) -> (u32, u32) {
        match tcp_stream.0.read_vectored(iovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(e) => (int_of_io_error(&e), 0),
        }
    }

    fn close_tcp_listener(&mut self, id: u32) {
        self.listeners.remove(id);
    }

    fn close_tcp_stream(&mut self, id: u32) {
        self.streams.remove(id);
    }

    fn tcp_stream_serialize(&self, tcp_stream: TcpStream) -> u32 {
        self.channel_state.serialize_host_resource(tcp_stream) as u32
    }

    fn tcp_stream_deserialize(&self, index: u32) -> TcpStreamResult {
        match self.channel_state.deserialize_host_resource(index as usize) {
            Some(tcp_stream) => TcpStreamResult::Ok(tcp_stream),
            None => TcpStreamResult::Err(format!(
                "No TcpStream found under index: {}, while deserializing",
                index
            )),
        }
    }

    fn tcp_listener_serialize(&self, tcp_listener: TcpListener) -> u32 {
        self.channel_state.serialize_host_resource(tcp_listener) as u32
    }

    fn tcp_listener_deserialize(&self, index: u32) -> TcpListenerResult {
        match self.channel_state.deserialize_host_resource(index as usize) {
            Some(tcp_listener) => TcpListenerResult::Ok(tcp_listener),
            None => TcpListenerResult::Err(format!(
                "No TcpStream found under index: {}, while deserializing",
                index
            )),
        }
    }
}

/// Returns a number corresponding to the `ErrorKind` of an `io::Error`. Always returns a number greater
/// than or equal to 1 (0 is used to indicate that the operation was successful.)
fn int_of_io_error(e: &io::Error) -> u32 {
    match e.kind() {
        io::ErrorKind::NotFound => 1,
        io::ErrorKind::PermissionDenied => 2,
        io::ErrorKind::ConnectionRefused => 3,
        io::ErrorKind::ConnectionReset => 4,
        io::ErrorKind::ConnectionAborted => 5,
        io::ErrorKind::NotConnected => 6,
        io::ErrorKind::AddrInUse => 7,
        io::ErrorKind::AddrNotAvailable => 8,
        io::ErrorKind::BrokenPipe => 9,
        io::ErrorKind::AlreadyExists => 10,
        io::ErrorKind::WouldBlock => 11,
        io::ErrorKind::InvalidInput => 12,
        io::ErrorKind::InvalidData => 13,
        io::ErrorKind::TimedOut => 14,
        io::ErrorKind::WriteZero => 15,
        io::ErrorKind::Interrupted => 16,
        io::ErrorKind::UnexpectedEof => 17,
        io::ErrorKind::Unsupported => 18,
        io::ErrorKind::OutOfMemory => 19,
        io::ErrorKind::Other => 20,
    }
}
