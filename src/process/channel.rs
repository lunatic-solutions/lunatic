use std::alloc::{alloc, dealloc, Layout};
use std::future::Future;
use std::{mem, ptr};

use smol::channel::{bounded, unbounded, Receiver, RecvError, Sender};

#[derive(Clone, Debug)]
pub struct Channel {
    sender: Sender<ChannelBuffer>,
    receiver: Receiver<ChannelBuffer>,
}

impl Channel {
    pub fn new(bound: Option<usize>) -> Self {
        let (sender, receiver) = match bound {
            Some(bound) => bounded(bound),
            None => unbounded(),
        };
        Self { sender, receiver }
    }

    pub fn send(&self, slice: &[u8]) -> impl Future + '_ {
        let buffer = ChannelBuffer::new(slice.as_ptr(), slice.len());
        self.sender.send(buffer)
    }

    pub fn recieve(&self) -> impl Future<Output = Result<ChannelBuffer, RecvError>> + '_ {
        self.receiver.recv()
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
}
