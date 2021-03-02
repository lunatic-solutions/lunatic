use super::resolver::Resolver;
use super::tcp::{TcpListener, TcpStream};
use uptown_funk::{state::HashMapStore, StateMarker};

use crate::api::channel::api::ChannelState;

pub struct TcpState {
    pub channel_state: ChannelState,
    pub resolvers: HashMapStore<Resolver>,
    pub listeners: HashMapStore<TcpListener>,
    pub streams: HashMapStore<TcpStream>,
}

impl StateMarker for TcpState {}

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
