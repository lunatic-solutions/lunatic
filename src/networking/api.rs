use super::{TcpListener, TcpListenerResult, TcpStream, TcpStreamResult};
use anyhow::Result;
use smol::prelude::*;
use uptown_funk::{host_functions, state::HashMapStore, StateMarker};

use crate::channel::api::ChannelState;

use std::io::{IoSlice, IoSliceMut};

pub struct TcpState {
    channel_state: ChannelState,
    pub listeners: HashMapStore<TcpListener>,
    pub streams: HashMapStore<TcpStream>,
}

impl StateMarker for TcpState {}

impl TcpState {
    pub fn new(channel_state: ChannelState) -> Self {
        Self {
            channel_state,
            listeners: HashMapStore::new(),
            streams: HashMapStore::new(),
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl TcpState {
    async fn tcp_bind(&self, address: &[u8], port: u32) -> (u32, TcpListenerResult) {
        match TcpListener::bind(address, port as u16).await {
            Ok(listener) => (0, TcpListenerResult::Ok(listener)),
            Err(err) => (1, TcpListenerResult::Err(err.to_string())),
        }
    }

    async fn tcp_accept(&self, tcp_listener: TcpListener) -> (u32, TcpStreamResult) {
        match tcp_listener.accept().await {
            Ok(stream) => (0, TcpStreamResult::Ok(stream)),
            Err(err) => (1, TcpStreamResult::Err(err.to_string())),
        }
    }

    async fn tcp_connect(&self, address: &[u8], port: u32) -> (u32, TcpStreamResult) {
        match TcpStream::connect(address, port as u16).await {
            Ok(tcp_stream) => (0, TcpStreamResult::Ok(tcp_stream)),
            Err(err) => (1, TcpStreamResult::Err(err.to_string())),
        }
    }

    async fn tcp_write_vectored(
        &self,
        mut tcp_stream: TcpStream,
        ciovs: &[IoSlice<'_>],
    ) -> (u32, u32) {
        match tcp_stream.0.write_vectored(ciovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(_) => (1, 0),
        }
    }

    async fn tcp_flush(&self, mut tcp_stream: TcpStream) -> u32 {
        match tcp_stream.0.flush().await {
            Ok(()) => 0,
            Err(_) => 1,
        }
    }

    async fn tcp_read_vectored<'a>(
        &self,
        tcp_stream: &'a mut TcpStream,
        iovs: &'a mut [IoSliceMut<'a>],
    ) -> (u32, u32) {
        match tcp_stream.0.read_vectored(iovs).await {
            Ok(bytes_written) => (0, bytes_written as u32),
            Err(_) => (1, 0),
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
