pub mod api;

use std::cell::RefCell;
use std::sync::atomic::AtomicUsize;

use dashmap::DashMap;
use lazy_static::lazy_static;
use smol::net::TcpStream;

lazy_static! {
    static ref SERIALIZED_TCP_STREAM: DashMap<usize, TcpStream> = DashMap::new();
}

static mut UNIQUE_ID: AtomicUsize = AtomicUsize::new(0);
