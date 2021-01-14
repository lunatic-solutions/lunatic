pub mod api;

use std::io;
use std::sync::atomic::AtomicUsize;

use dashmap::DashMap;
use lazy_static::lazy_static;
use uptown_funk::{Executor, FromWasm, ToWasm};

lazy_static! {
    static ref SERIALIZED_TCP_STREAM: DashMap<usize, TcpStream> = DashMap::new();
}

static mut UNIQUE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct TcpListener(smol::net::TcpListener);

impl TcpListener {
    pub async fn bind(address: &str) -> Result<Self, io::Error> {
        match smol::net::TcpListener::bind(address).await {
            Ok(tcp_listener) => Ok(Self(tcp_listener)),
            Err(err) => Err(err),
        }
    }

    pub async fn accept(&self) -> Result<TcpStream, io::Error> {
        let (stream, address) = self.0.accept().await?;
        Ok(TcpStream { stream, address })
    }
}

impl FromWasm for TcpListener {
    type From = u32;
    type State = api::TcpState;

    fn from(
        state: &mut Self::State,
        _: &impl Executor,
        tcp_listener_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.listeners.get(tcp_listener_id) {
            Some(tcp_listener) => Ok(tcp_listener.clone()),
            None => Err(uptown_funk::Trap::new("TcpListener not found")),
        }
    }
}

enum TcpListenerResult {
    Ok(TcpListener),
    Err(io::Error),
}

impl ToWasm for TcpListenerResult {
    type To = u32;
    type State = api::TcpState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            TcpListenerResult::Ok(listener) => Ok(state.listeners.add(listener)),
            TcpListenerResult::Err(_) => Ok(0),
        }
    }
}

#[derive(Clone)]
pub struct TcpStream {
    stream: smol::net::TcpStream,
    address: smol::net::SocketAddr,
}

impl FromWasm for TcpStream {
    type From = u32;
    type State = api::TcpState;

    fn from(
        state: &mut Self::State,
        _: &impl Executor,
        tcp_stream_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.streams.get(tcp_stream_id) {
            Some(tcp_stream) => Ok(tcp_stream.clone()),
            None => Err(uptown_funk::Trap::new("TcpStream not found")),
        }
    }
}
enum TcpStreamResult {
    Ok(TcpStream),
    Err(io::Error),
}

impl ToWasm for TcpStreamResult {
    type To = u32;
    type State = api::TcpState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            TcpStreamResult::Ok(stream) => Ok(state.streams.add(stream)),
            TcpStreamResult::Err(_) => Ok(0),
        }
    }
}
