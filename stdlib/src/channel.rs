use std::marker::PhantomData;
use std::mem::{forget, size_of, zeroed};

use crate::stdlib::{clone, drop};
use crate::ProcessClosureSend;

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
        pub fn send(channel: u32, data: *const __wasi_iovec_t);
        pub fn receive(channel: u32, data: *const __wasi_iovec_t);
    }
}

pub struct Channel<T> {
    id: u32,
    phantom: PhantomData<T>,
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        // Increment reference count of resource in the VM
        unsafe {
            clone(self.id);
        }
        Self {
            id: self.id,
            phantom: PhantomData,
        }
    }
}

impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        // Decrement reference count of resource in the VM
        unsafe {
            drop(self.id);
        }
    }
}

impl<T: ProcessClosureSend> Channel<T> {
    /// If `bound` is 0, returns an unbound channel.
    pub fn new(bound: usize) -> Self {
        let id = unsafe { stdlib::channel(bound as u32) };
        Self {
            id,
            phantom: PhantomData,
        }
    }

    pub fn send(&self, value: T) {
        let data = __wasi_iovec_t {
            buf: &value as *const T as u32,
            buf_len: size_of::<T>() as u32,
        };

        unsafe {
            stdlib::send(self.id, &data as *const __wasi_iovec_t);
        }
        forget(value);
    }

    pub fn receive(&self) -> T {
        let result: T = unsafe { zeroed() };

        let data = __wasi_iovec_t {
            buf: &result as *const T as u32,
            buf_len: size_of::<T>() as u32,
        };

        unsafe {
            stdlib::receive(self.id, &data as *const __wasi_iovec_t);
        }
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
}
