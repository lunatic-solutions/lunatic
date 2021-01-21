use super::{TcpListener, TcpListenerResult, TcpStream, TcpStreamResult};
use anyhow::Result;
use smol::prelude::*;
use uptown_funk::{host_functions, state::HashMapStore, StateMarker};

use std::{
    io::{IoSlice, IoSliceMut},
    sync::atomic::Ordering,
};

pub struct TcpState {
    pub listeners: HashMapStore<TcpListener>,
    pub streams: HashMapStore<TcpStream>,
}

impl StateMarker for TcpState {}

impl TcpState {
    pub fn new() -> Self {
        Self {
            listeners: HashMapStore::new(),
            streams: HashMapStore::new(),
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl TcpState {
    async fn tcp_bind_str(&self, address: &str) -> (u32, TcpListenerResult) {
        match TcpListener::bind(address).await {
            Ok(listener) => (0, TcpListenerResult::Ok(listener)),
            Err(err) => (1, TcpListenerResult::Err(err)),
        }
    }

    async fn tcp_accept(&self, tcp_listener: TcpListener) -> (u32, TcpStreamResult) {
        match tcp_listener.accept().await {
            Ok(stream) => (0, TcpStreamResult::Ok(stream)),
            Err(err) => (1, TcpStreamResult::Err(err)),
        }
    }

    async fn tcp_write_vectored(
        &self,
        mut tcp_stream: TcpStream,
        ciovs: &[IoSlice<'_>],
    ) -> (u32, u32) {
        match tcp_stream.stream.write_vectored(ciovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(_) => (1, 0),
        }
    }

    async fn tcp_read_vectored<'a>(
        &self,
        tcp_stream: &'a mut TcpStream,
        iovs: &'a mut [IoSliceMut<'a>],
    ) -> (u32, u32) {
        match tcp_stream.stream.read_vectored(iovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(_) => (1, 0),
        }
    }

    // Serializes an Externref containing a tcp_stream as an id.
    // Memory leak: If the value in never deserialized, this will leak memory.
    async fn tcp_stream_serialize(&self, tcp_stream: TcpStream) -> i64 {
        let id = unsafe { super::UNIQUE_ID.fetch_add(1, Ordering::SeqCst) };
        super::SERIALIZED_TCP_STREAM.insert(id, tcp_stream);
        id as i64
    }

    async fn tcp_stream_deserialize(&self, serialized_tcp_stream: i64) -> TcpStreamResult {
        match super::SERIALIZED_TCP_STREAM.remove(&(serialized_tcp_stream as usize)) {
            Some((_id, stream)) => TcpStreamResult::Ok(stream),
            None => panic!("Can't deserialize tcp stream"),
        }
    }
}
