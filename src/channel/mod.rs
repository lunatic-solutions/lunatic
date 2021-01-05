//! Channels allow for sending data between processes.
//!
//! Two processes don't share any memory and the only way of communicating with each other is through
//! messages. All data sent from one process to another is first copied from the heap of the source
//! process to the `ChannelBuffer` and then from the buffer to the heap of the receiving process.

pub mod api;

use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::{mem, ptr};

use dashmap::{mapref::entry::Entry, DashMap};
use lazy_static::lazy_static;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use smol::channel::{bounded, unbounded, Receiver, RecvError, Sender};
use uptown_funk::{FromWasmU32, ToWasmU32};

lazy_static! {
    static ref CHANNELS: DashMap<u32, Channel> = DashMap::new();
}

thread_local! {
    static RNG: RefCell<SmallRng> = RefCell::new(SmallRng::from_entropy());
}

// Assigns a random id to the `channel` and adds it to the global CHANNELS map.
fn add_channel(channel: Channel) -> u32 {
    let id = RNG.with(|rng| rng.borrow_mut().gen());
    match CHANNELS.entry(id) {
        Entry::Vacant(ve) => ve.insert(channel),
        Entry::Occupied(_) => return add_channel(channel), // try another id
    };
    id
}

// Remove channel by id from global CHANNELS map.
fn remove_channel(id: u32) {
    CHANNELS.remove(&id);
}

#[derive(Clone)]
pub struct Channel {
    sender: Sender<ChannelBuffer>,
    receiver: Receiver<ChannelBuffer>,
}

impl ToWasmU32 for Channel {
    type State = api::ChannelState;

    fn to_u32<ProcessEnvironment>(
        _: &mut Self::State,
        _: &ProcessEnvironment,
        channel: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(add_channel(channel) as u32)
    }
}

impl FromWasmU32 for Channel {
    type State = api::ChannelState;

    fn from_u32<ProcessEnvironment>(
        _: &mut Self::State,
        _: &ProcessEnvironment,
        id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match CHANNELS.get(&id) {
            Some(channel) => Ok(channel.clone()),
            None => Err(uptown_funk::Trap::new("Channel not found")),
        }
    }
}

impl Channel {
    pub fn new(bound: Option<usize>) -> Self {
        let (sender, receiver) = match bound {
            Some(bound) => bounded(bound),
            None => unbounded(),
        };
        Self { sender, receiver }
    }

    pub async fn send(&self, slice: &[u8]) {
        let buffer = ChannelBuffer::new(slice.as_ptr(), slice.len());
        self.sender.send(buffer).await.unwrap();
    }

    pub async fn receive(&self) -> Result<ChannelBuffer, RecvError> {
        self.receiver.recv().await
    }
}

pub struct ChannelBuffer {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for ChannelBuffer {}

impl ChannelBuffer {
    pub fn new(source: *const u8, len: usize) -> Self {
        unsafe {
            let layout = Layout::from_size_align(len, 16).expect("Invalid layout");
            let ptr: *mut u8 = mem::transmute(alloc(layout));
            ptr::copy_nonoverlapping(source, ptr, len);
            Self { ptr, len }
        }
    }

    pub fn give_to(self, destination: *mut u8) {
        unsafe {
            ptr::copy_nonoverlapping(self.ptr, destination, self.len);
            let layout = Layout::from_size_align(self.len, 16).expect("Invalid layout");
            dealloc(self.ptr, layout)
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
