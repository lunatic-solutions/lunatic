use std::alloc::{alloc, dealloc, Layout};
use std::fmt;
use std::marker::PhantomData;
use std::mem::{forget, transmute};
use std::slice;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

use crate::drop;

#[repr(C)]
pub struct __wasi_iovec_t {
    pub buf: u32,
    pub buf_len: u32,
}

mod stdlib {
    use super::__wasi_iovec_t;

    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn channel(bound: u32) -> u32;
        pub fn channel_send(channel: u32, data: *const __wasi_iovec_t);
        pub fn channel_receive(
            channel: u32,
            allocation_function: unsafe extern "C" fn(usize) -> *mut u8,
            buf: *mut usize,
        ) -> usize;
        pub fn channel_serialize(channel: u32) -> u64;
        pub fn channel_deserialize(channel: u64) -> u32;
    }
}

/// A channel allows exchanging messages between processes.
/// The message needs to implement `serde::ser::Serializer`, because processes don't share any memory.
#[derive(Clone)]
pub struct Channel<T> {
    id: u32,
    phantom: PhantomData<T>,
}

impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        drop(self.id);
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Channel<T> {
    /// If `bound` is 0, returns an unbound channel.
    pub fn new(bound: usize) -> Self {
        let id = unsafe { stdlib::channel(bound as u32) };
        Self {
            id,
            phantom: PhantomData,
        }
    }

    pub fn send(&self, value: T) {
        let value_serialized = bincode::serialize(&value).unwrap();
        let data = __wasi_iovec_t {
            buf: value_serialized.as_ptr() as u32,
            buf_len: value_serialized.len() as u32,
        };

        unsafe {
            stdlib::channel_send(self.id, &data as *const __wasi_iovec_t);
        }
    }

    pub fn receive(&self) -> T {
        unsafe extern "C" fn allocation_function(size: usize) -> *mut u8 {
            let layout = Layout::from_size_align(size, 8).expect("Invalid layout");
            transmute(alloc(layout))
        }

        let mut buf_ptr: usize = 0;
        let serialized_buffer = unsafe {
            let buf_len =
                stdlib::channel_receive(self.id, allocation_function, &mut buf_ptr as *mut usize);
            slice::from_raw_parts(buf_ptr as *const u8, buf_len)
        };

        let result: T = bincode::deserialize(serialized_buffer).unwrap();

        let layout = Layout::from_size_align(serialized_buffer.len(), 8).expect("Invalid layout");
        unsafe { dealloc(buf_ptr as *mut u8, layout) };

        result
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub unsafe fn from_id(id: u32) -> Self {
        Self {
            id,
            phantom: PhantomData,
        }
    }

    pub fn serialize_as_u64(self) -> u64 {
        unsafe { stdlib::channel_serialize(self.id) }
    }

    pub fn dserialize_from_u64(id: u64) -> Self {
        let channel_id = unsafe { stdlib::channel_deserialize(id) };
        unsafe { Channel::from_id(channel_id) }
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Serialize for Channel<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let serialized_channel = unsafe { stdlib::channel_serialize(self.id) };
        serializer.serialize_u64(serialized_channel)
    }
}

struct ChannelVisitor<T> {
    phantom: PhantomData<T>,
}

impl<'de, T: Serialize + Deserialize<'de>> Visitor<'de> for ChannelVisitor<T> {
    type Value = Channel<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an pointer to an externref containing a channel")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let channel_id = unsafe { stdlib::channel_deserialize(value) };
        unsafe { Ok(Channel::from_id(channel_id)) }
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Deserialize<'de> for Channel<T> {
    fn deserialize<D>(deserializer: D) -> Result<Channel<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_u64(ChannelVisitor {
            phantom: PhantomData,
        })
    }
}
