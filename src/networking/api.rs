use super::{TcpListener, TcpStream};
use anyhow::Result;
use uptown_funk::host_functions;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

pub struct TcpListenerState {
    count_listener: RefCell<i32>,
    state_listener: RefCell<HashMap<i32, TcpListener>>,
    count_stream: RefCell<i32>,
    state_stream: RefCell<HashMap<i32, TcpStream>>,
}

impl TcpListenerState {
    pub fn new() -> Self {
        Self {
            count_listener: RefCell::new(0),
            state_listener: RefCell::new(HashMap::new()),
            count_stream: RefCell::new(0),
            state_stream: RefCell::new(HashMap::new()),
        }
    }

    pub fn add_tcp_listener(&self, listener: TcpListener) -> i32 {
        let mut id = self.count_listener.borrow_mut();
        *id += 1;
        self.state_listener.borrow_mut().insert(*id, listener);
        *id
    }

    pub fn remove_tcp_listener(&self, id: i32) -> Option<TcpListener> {
        self.state_listener.borrow_mut().remove(&id)
    }

    pub fn add_tcp_stream(&self, stream: TcpStream) -> i32 {
        let mut id = self.count_stream.borrow_mut();
        *id += 1;
        self.state_stream.borrow_mut().insert(*id, stream);
        *id
    }

    pub fn remove_tcp_stream(&self, id: i32) -> Option<TcpStream> {
        self.state_stream.borrow_mut().remove(&id)
    }
}

#[host_functions(namespace = "lunatic")]
impl TcpListenerState {
    async fn tcp_bind_str(&self, address: &str) -> (i32, TcpListener) {
        match TcpListener::bind(address).await {
            Ok(tcp_listener) => (0, tcp_listener),
            Err(tcp_listener) => (-1, tcp_listener),
        }
    }

    async fn tcp_accept(&self, tcp_listener: TcpListener) -> (i32, TcpStream) {
        (0, tcp_listener.accept().await)
    }

    // Serializes an Externref containing a tcp_stream as an id.
    // Memory leak: If the value in never deserialized, this will leak memory.
    async fn tcp_stream_serialize(&self, tcp_stream: TcpStream) -> i64 {
        let id = unsafe { super::UNIQUE_ID.fetch_add(1, Ordering::SeqCst) };
        super::SERIALIZED_TCP_STREAM.insert(id, tcp_stream);
        id as i64
    }

    async fn tcp_stream_deserialize(&self, serialized_tcp_stream: i64) -> TcpStream {
        match super::SERIALIZED_TCP_STREAM.remove(&(serialized_tcp_stream as usize)) {
            Some((_id, tcp_stream)) => tcp_stream,
            None => panic!("Can't deserialize tcp stream"),
        }
    }
}
