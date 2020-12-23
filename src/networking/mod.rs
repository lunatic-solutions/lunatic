pub mod api;

use std::sync::atomic::AtomicUsize;

use dashmap::DashMap;
use lazy_static::lazy_static;
use uptown_funk::{FromWasmI32, ToWasmI32};

lazy_static! {
    static ref SERIALIZED_TCP_STREAM: DashMap<usize, TcpStream> = DashMap::new();
}

static mut UNIQUE_ID: AtomicUsize = AtomicUsize::new(0);

pub enum TcpListener {
    Smol(smol::net::TcpListener),
    Err,
}

impl TcpListener {
    pub async fn bind(address: &str) -> Result<Self, Self> {
        match smol::net::TcpListener::bind(address).await {
            Ok(tcp_listener) => Ok(Self::Smol(tcp_listener)),
            Err(_) => Err(Self::Err),
        }
    }

    pub async fn accept(&self) -> TcpStream {
        match self {
            TcpListener::Smol(tcp_listerner) => TcpStream {
                _inner: tcp_listerner.accept().await.unwrap().0,
            },
            TcpListener::Err => panic!("Can't accept on TcpListener"),
        }
    }
}

impl ToWasmI32 for TcpListener {
    type State = api::TcpListenerState;

    fn to_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        tcp_listener: Self,
    ) -> Result<i32, uptown_funk::Trap> {
        Ok(state.add_tcp_listener(tcp_listener))
    }
}

impl FromWasmI32 for TcpListener {
    type State = api::TcpListenerState;

    fn from_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        id: i32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.remove_tcp_listener(id) {
            Some(tcp_listener) => Ok(tcp_listener),
            None => Err(uptown_funk::Trap::new("TcpListener not found")),
        }
    }
}

pub struct TcpStream {
    _inner: smol::net::TcpStream,
}

impl ToWasmI32 for TcpStream {
    type State = api::TcpListenerState;

    fn to_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        tcp_stream: Self,
    ) -> Result<i32, uptown_funk::Trap> {
        Ok(state.add_tcp_stream(tcp_stream))
    }
}

impl FromWasmI32 for TcpStream {
    type State = api::TcpListenerState;

    fn from_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        id: i32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.remove_tcp_stream(id) {
            Some(tcp_stream) => Ok(tcp_stream),
            None => Err(uptown_funk::Trap::new("TcpListener not found")),
        }
    }
}
