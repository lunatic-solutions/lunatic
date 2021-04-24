use super::{
    host_resources::Resource, ChannelReceiver, ChannelReceiverResult, ChannelSender,
    ChannelSenderResult, Message,
};

use anyhow::Result;
use smol::channel::{bounded, unbounded};
use uptown_funk::{host_functions, state::HashMapStore};

use std::{
    cell::RefCell,
    convert::{TryFrom, TryInto},
    mem::replace,
    rc::Rc,
};

#[derive(Clone)]
pub struct ChannelState {
    pub inner: Rc<RefCell<InnerChannelState>>,
}

/// Host resources need to be sent separately to another instance, because they can't be serialized on
/// the guest. This is done by adding them to `next_message_host_resources` during the serialization
/// ont the guest and just serializing it as the index of the resource in this array.
///
/// Smol's channels don't allow you to peek into the message without consuming it, but Lunatic's API
/// requires us to check the size of the next message, so that the guest side can allocate a big enough
/// buffer. Because of this the `last_received_message` is temporarily saved inside the instance state
/// before it's completely consumed.
pub struct InnerChannelState {
    pub senders: HashMapStore<ChannelSender>,
    pub receivers: HashMapStore<ChannelReceiver>,
    pub next_message_host_resources: Vec<Resource>,
    last_received_message: Option<Message>,
}

impl<'a> ChannelState {
    pub fn new(context_receiver: Option<ChannelReceiver>) -> Self {
        let mut receivers = HashMapStore::new();
        if let Some(context_receiver) = context_receiver {
            receivers.add(context_receiver);
        }
        let inner = InnerChannelState {
            senders: HashMapStore::new(),
            receivers,
            next_message_host_resources: Vec::new(),
            last_received_message: None,
        };
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    /// Prepares the host resource for sending by adding it to the next message.
    /// Returns the index inside the message's host resource vec.
    pub fn serialize_host_resource<R>(&self, resource: R) -> usize
    where
        R: Into<Resource>,
    {
        let resources = &mut self.inner.borrow_mut().next_message_host_resources;
        let index = resources.len();
        resources.push(resource.into());
        index
    }

    /// Extracts the resource from the last received message.
    /// Returns None if:
    /// - No message was received
    /// - No resource exists under this index
    /// - The resource is of different type
    pub fn deserialize_host_resource<R>(&self, index: usize) -> Option<R>
    where
        R: TryFrom<Resource>,
    {
        let mut inner = self.inner.borrow_mut();
        let message = match &mut inner.last_received_message {
            Some(message) => message,
            None => return None,
        };

        match message.host_resources.get_mut(index) {
            Some(resource) => {
                let resource = replace(resource, Resource::Empty);
                match resource.try_into() {
                    Ok(resource) => Some(resource),
                    Err(_) => None,
                }
            }
            None => None,
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl ChannelState {
    // Create a new channel
    fn channel(&self, bound: u32) -> (ChannelSender, ChannelReceiver) {
        if bound > 0 {
            let (sender, receiver) = bounded(bound as usize);
            (ChannelSender(sender), ChannelReceiver(receiver))
        } else {
            let (sender, receiver) = unbounded();
            (ChannelSender(sender), ChannelReceiver(receiver))
        }
    }

    // Remove a sender
    fn close_sender(&self, id: u32) {
        self.inner.borrow_mut().senders.remove(id);
    }

    // Remove receiver
    fn close_receiver(&self, id: u32) {
        self.inner.borrow_mut().receivers.remove(id);
    }

    /// Drops last message
    fn drop_last_message(&self) {
        self.inner.borrow_mut().last_received_message.take();
    }

    /// Sends a message to channel.
    /// The message consists of 2 parts:
    /// - A serialized binary buffer passed by the host, with references to host resources
    /// - Host resources held by the instance
    ///
    /// Returns 0 if successful, otherwise 1
    async fn channel_send(&self, channel: ChannelSender, buffer: &[u8]) -> u32 {
        let resources = &mut self.inner.borrow_mut().next_message_host_resources;
        let host_resources = replace(resources, Vec::new());
        match channel.send(buffer, host_resources).await {
            Ok(_) => 0,
            Err(_) => 1,
        }
    }

    /// Writes the last prepared message to the `iovec_slice.`
    /// Needs to be called after `channel_receive_prepare`.
    //
    /// Returns 0 if successful, otherwise 1
    async fn channel_receive(&mut self, buffer: &mut [u8]) -> u32 {
        match &mut self.inner.borrow_mut().last_received_message {
            Some(channel_buffer) => {
                channel_buffer.write_to(buffer.as_mut_ptr());
                0
            }
            None => 1,
        }
    }

    /// Blocks until a message is received, then stores the message in the `last_received_message` field.
    ///
    /// Returns a tuple (error_code, message_size)
    /// error_code - 0 if successful, otherwise 1
    async fn channel_receive_prepare(&mut self, channel: ChannelReceiver) -> (u32, u32) {
        let message = channel.receive().await;
        match message {
            Ok(channel_buffer) => {
                let size = channel_buffer.len();
                self.inner
                    .borrow_mut()
                    .last_received_message
                    .replace(channel_buffer);
                (0, size as u32)
            }
            Err(_) => (1, 0),
        }
    }

    fn sender_serialize(&self, sender: ChannelSender) -> u32 {
        self.serialize_host_resource(sender) as u32
    }

    fn sender_deserialize(&self, index: u32) -> ChannelSenderResult {
        match self.deserialize_host_resource(index as usize) {
            Some(sender) => ChannelSenderResult::Ok(sender),
            None => ChannelSenderResult::Err(format!(
                "No ChannelSender found under index: {}, while deserializing",
                index
            )),
        }
    }

    fn receiver_serialize(&self, receiver: ChannelReceiver) -> u32 {
        self.serialize_host_resource(receiver) as u32
    }

    fn receiver_deserialize(&self, index: u32) -> ChannelReceiverResult {
        match self.deserialize_host_resource(index as usize) {
            Some(receiver) => ChannelReceiverResult::Ok(receiver),
            None => ChannelReceiverResult::Err(format!(
                "No ChannelReceiverResult found under index: {}, while deserializing",
                index
            )),
        }
    }
}
