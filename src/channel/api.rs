use super::Channel;

use anyhow::Result;
use uptown_funk::host_functions;

use core::panic;
use std::io::{IoSlice, IoSliceMut};
use std::{cell::RefCell, collections::HashMap};

pub struct ChannelState {
    count: RefCell<i32>,
    pub state: RefCell<HashMap<i32, Channel>>,
}

impl ChannelState {
    pub fn new() -> Self {
        Self {
            count: RefCell::new(0),
            state: RefCell::new(HashMap::new()),
        }
    }

    pub fn add_channel(&self, channel: Channel) -> i32 {
        let mut id = self.count.borrow_mut();
        *id += 1;
        self.state.borrow_mut().insert(*id, channel);
        *id
    }

    pub fn remove_channel(&self, id: i32) -> Option<Channel> {
        self.state.borrow_mut().remove(&id)
    }
}

#[host_functions(namespace = "lunatic")]
impl ChannelState {
    fn channel(&self, bound: i32) -> Channel {
        Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        })
    }

    fn channel_serialize(&self, channel: Channel) -> i64 {
        channel.serialize() as i64
    }

    fn channel_deserialize(&self, maybe_channel: i64) -> Channel {
        match Channel::deserialize(maybe_channel as usize) {
            Some(channel) => channel,
            None => panic!("Channel doesn't exist"),
        }
    }

    async fn channel_send(&self, channel: Channel, ciovec_slice: &[IoSlice<'_>]) {
        assert_eq!(ciovec_slice.len(), 1);
        if let Some(buffer) = ciovec_slice.first() {
            channel.send(buffer).await;
        }
    }

    async fn channel_receive(&self, channel: Channel, iovec_slice: &mut [IoSliceMut<'_>]) -> i32 {
        assert_eq!(iovec_slice.len(), 1);
        if let Some(guest_buffer) = iovec_slice.first_mut() {
            let buffer = channel.receive().await;
            match buffer {
                Ok(channel_buffer) => {
                    let length = channel_buffer.len();
                    channel_buffer.give_to(guest_buffer.as_mut_ptr());
                    return length as i32;
                }
                Err(_) => return -1,
            }
        };
        -1
    }

    async fn channel_next_message_size(&self, channel: Channel) -> i32 {
        match channel.next_message_size().await {
            Ok(size) => size as i32,
            Err(_) => -1,
        }
    }
}
