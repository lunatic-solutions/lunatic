pub mod api;

use std::io;
use std::sync::atomic::AtomicUsize;

use dashmap::DashMap;
use lazy_static::lazy_static;
use uptown_funk::{FromWasmU32, ToWasmU32};

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

impl<'a> FromWasmU32<'a> for TcpListener {
    type State = api::TcpState;

    fn from_u32<ProcessEnvironment>(
        state: &mut Self::State,
        _: &ProcessEnvironment,
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

impl ToWasmU32 for TcpListenerResult {
    type State = api::TcpState;

    fn to_u32<ProcessEnvironment>(
        state: &mut Self::State,
        _: &ProcessEnvironment,
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

impl<'a> FromWasmU32<'a> for TcpStream {
    type State = api::TcpState;

    fn from_u32<ProcessEnvironment>(
        state: &mut Self::State,
        _instance_environment: &ProcessEnvironment,
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

impl ToWasmU32 for TcpStreamResult {
    type State = api::TcpState;

    fn to_u32<ProcessEnvironment>(
        state: &mut Self::State,
        _instance_environment: &ProcessEnvironment,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            TcpStreamResult::Ok(stream) => Ok(state.streams.add(stream)),
            TcpStreamResult::Err(_) => Ok(0),
        }
    }
}
