use std::alloc::{alloc, dealloc, Layout};
use std::fmt;
use std::marker::PhantomData;
use std::mem::transmute;
use std::slice;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

use crate::{clone, drop, Externref};

// TODO: Replace with std::io::IoSlice. Same ABI!
#[repr(C)]
pub struct __wasi_iovec_t {
    pub buf: u32,
    pub buf_len: u32,
}

mod stdlib {
    use super::__wasi_iovec_t;
    use crate::Externref;

    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn channel(bound: u32) -> Externref;
        pub fn channel_send(channel: Externref, data: *const __wasi_iovec_t);
        pub fn channel_receive(
            channel: Externref,
            allocation_function: unsafe extern "C" fn(usize) -> *mut u8,
            buf: *mut usize,
        ) -> usize;
        pub fn channel_serialize(channel: Externref) -> u64;
        pub fn channel_deserialize(channel: u64) -> Externref;
    }
}

/// A channel allows exchanging messages between processes.
/// The message needs to implement `serde::ser::Serializer`, because processes don't share any memory.
pub struct Channel<T> {
    externref: Externref,
    phantom: PhantomData<T>,
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        let externref = clone(self.externref);
        Self {
            externref,
            phantom: PhantomData,
        }
    }
}

impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        drop(self.externref);
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Channel<T> {
    /// If `bound` is 0, returns an unbound channel.
    pub fn new(bound: usize) -> Self {
        let externref = unsafe { stdlib::channel(bound as u32) };
        Self {
            externref,
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
            stdlib::channel_send(self.externref, &data as *const __wasi_iovec_t);
        }
    }

    pub fn receive(&self) -> T {
        unsafe extern "C" fn allocation_function(size: usize) -> *mut u8 {
            let layout = Layout::from_size_align(size, 8).expect("Invalid layout");
            transmute(alloc(layout))
        }

        let mut buf_ptr: usize = 0;
        let serialized_buffer = unsafe {
            let buf_len = stdlib::channel_receive(
                self.externref,
                allocation_function,
                &mut buf_ptr as *mut usize,
            );
            slice::from_raw_parts(buf_ptr as *const u8, buf_len)
        };

        let result: T = bincode::deserialize(serialized_buffer).unwrap();

        let layout = Layout::from_size_align(serialized_buffer.len(), 8).expect("Invalid layout");
        unsafe { dealloc(buf_ptr as *mut u8, layout) };

        result
    }

    pub fn externref(&self) -> Externref {
        self.externref
    }

    pub unsafe fn from_externref(externref: Externref) -> Self {
        Self {
            externref,
            phantom: PhantomData,
        }
    }

    pub fn serialize_as_u64(self) -> u64 {
        unsafe { stdlib::channel_serialize(self.externref) }
    }

    pub fn deserialize_from_u64(id: u64) -> Self {
        let channel_externref = unsafe { stdlib::channel_deserialize(id) };
        unsafe { Channel::from_externref(channel_externref) }
    }
}

impl<'de, T: Serialize + Deserialize<'de>> Serialize for Channel<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let serialized_channel = unsafe { stdlib::channel_serialize(self.externref) };
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
        let channel_externref = unsafe { stdlib::channel_deserialize(value) };
        unsafe { Ok(Channel::from_externref(channel_externref)) }
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
