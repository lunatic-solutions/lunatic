//! Channels allow for sending data between processes.
//!
//! Two processes don't share any memory and the only way of communicating with each other is through
//! messages. All data sent from one process to another is first copied from the heap of the source
//! process to the `ChannelBuffer` and then from the buffer to the heap of the receiving process.

pub mod api;

use std::alloc::{alloc, dealloc, Layout};
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{mem, ptr};

use dashmap::DashMap;
use lazy_static::lazy_static;
use smol::channel::{bounded, unbounded, Receiver, RecvError, Sender};
use uptown_funk::{FromWasmI32, ToWasmI32};

lazy_static! {
    // Channels are used to send messages between processes, but they can also be sent themselves
    // between processes. This causes somewhat of a chicken and egg problem. The only values that
    // we can easily be pass between processes are WASM primitive types (i32, i64). One solution
    // to overcome this issue would be to wrap the channel in an wasmtime::Externref and give it
    // to another process, but Wasmtime's Externref's are not Sync or Send and can't be passed to
    // other processes safely (that may run on different threads).
    //
    // To overcome this issue, when we serialize a Channel it is first added to this collection
    // and only the id (i64) is passed to the new process. The new process will take it out of the
    // collection during deserialization.
    //
    // Memory leaks:
    // If a process serializes a channel, but it never gets deserialized (e.g. receiving process dies)
    // the channel will stay forever in this collection. Causing a memory leak.
    static ref SERIALIZED_CHANNELS: DashMap<usize, Channel> = DashMap::new();
}

static mut UNIQUE_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Channel {
    id: usize,
    sender: Sender<ChannelBuffer>,
    sender_len: Sender<usize>,
    receiver: Receiver<ChannelBuffer>,
    receiver_len: Receiver<usize>,
}

impl ToWasmI32 for Channel {
    type State = api::ChannelState;

    fn to_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        channel: Self,
    ) -> Result<i32, uptown_funk::Trap> {
        Ok(state.add_channel(channel))
    }
}

impl FromWasmI32 for Channel {
    type State = api::ChannelState;

    fn from_i32<ProcessEnvironment>(
        state: &Self::State,
        _instance_environment: &ProcessEnvironment,
        id: i32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.remove_channel(id) {
            Some(channel) => Ok(channel),
            None => Err(uptown_funk::Trap::new("Channel not found")),
        }
    }
}

impl Clone for Channel {
    fn clone(&self) -> Self {
        Self {
            id: unsafe { UNIQUE_ID.fetch_add(1, Ordering::SeqCst) },
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            sender_len: self.sender_len.clone(),
            receiver_len: self.receiver_len.clone(),
        }
    }
}

impl Channel {
    pub fn new(bound: Option<usize>) -> Self {
        let id = unsafe { UNIQUE_ID.fetch_add(1, Ordering::SeqCst) };
        let (sender, receiver) = match bound {
            Some(bound) => bounded(bound),
            None => unbounded(),
        };
        let (sender_len, receiver_len) = match bound {
            Some(bound) => bounded(bound),
            None => unbounded(),
        };
        Self {
            id,
            sender,
            sender_len,
            receiver,
            receiver_len,
        }
    }

    pub async fn send(&self, slice: &[u8]) {
        let buffer = ChannelBuffer::new(slice.as_ptr(), slice.len());
        self.sender_len.send(buffer.len()).await.unwrap();
        self.sender.send(buffer).await.unwrap();
    }

    pub fn receive(&self) -> impl Future<Output = Result<ChannelBuffer, RecvError>> + '_ {
        self.receiver.recv()
    }

    // TODO: This must be called right before receive & the same exact number of times.
    // There should be a better design.
    pub fn next_message_size(&self) -> impl Future<Output = Result<usize, RecvError>> + '_ {
        self.receiver_len.recv()
    }

    pub fn serialize(self) -> usize {
        let id = self.id;
        SERIALIZED_CHANNELS.insert(id, self);
        id
    }

    pub fn deserialize(id: usize) -> Option<Channel> {
        match SERIALIZED_CHANNELS.remove(&id) {
            Some((_id, channel)) => Some(channel),
            None => None,
        }
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
