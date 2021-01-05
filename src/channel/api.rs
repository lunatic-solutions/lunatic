use super::{remove_channel, Channel, ChannelBuffer};

use anyhow::Result;
use uptown_funk::host_functions;

use std::io::{IoSlice, IoSliceMut};

/// Smol's channels don't allow you to peek into the message without consuming it, but Lunatic's API
/// requires us to check the size of the next message, so that the guest side can allocate a big enough
/// buffer. Becuase of this the last message is temporarily saved inside the instance state before it's
// completely consumed.
pub struct ChannelState {
    last_message: Option<ChannelBuffer>,
}

impl ChannelState {
    pub fn new() -> Self {
        Self { last_message: None }
    }
}

#[host_functions(namespace = "lunatic")]
impl ChannelState {
    fn channel_open(&self, bound: i32) -> Channel {
        Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        })
    }

    fn channel_close(&self, id: u32) {
        remove_channel(id)
    }

    async fn channel_send(&self, channel: Channel, ciovec_slice: &[IoSlice<'_>]) {
        assert_eq!(ciovec_slice.len(), 1);
        if let Some(buffer) = ciovec_slice.first() {
            channel.send(buffer).await;
        }
    }

    /// Writes the last prepared message to the `iovec_slice.`
    /// Needs to be called after `channel_receive_prepare`.
    async fn channel_receive(&mut self, iovec_slice: &mut [IoSliceMut<'_>]) {
        assert_eq!(iovec_slice.len(), 1);
        if let Some(buffer) = iovec_slice.first_mut() {
            match self.last_message.take() {
                Some(channel_buffer) => channel_buffer.give_to(buffer.as_mut_ptr()),
                None => panic!("channel_receive_prepare must be called before"),
            }
        }
    }

    /// Blocks until a message is received, then stores the message in the `last_message` field.
    /// Returns the size of the message.
    async fn channel_receive_prepare(&mut self, channel: Channel) -> u32 {
        let message = channel.receive().await;
        match message {
            Ok(channel_buffer) => {
                let size = channel_buffer.len();
                self.last_message.replace(channel_buffer);
                size as u32
            }
            Err(_) => panic!("Channel is closed?"),
        }
    }
}
