use std::alloc::{alloc, dealloc, Layout};
use std::{mem, ptr};

pub struct ChannelBuffer {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for ChannelBuffer {}

impl ChannelBuffer {
    pub fn new(source: *mut u8, len: usize) -> Self {
        unsafe {
            let layout = Layout::from_size_align(len, 16).expect("Invalid layout");
            let ptr: *mut u8 = mem::transmute(alloc(layout));
            ptr::copy_nonoverlapping(source, ptr, len);
            Self { ptr, len }
        }
    }

    pub fn take(self, destination: *mut u8) {
        unsafe {
            ptr::copy_nonoverlapping(self.ptr, destination, self.len);
            let layout = Layout::from_size_align(self.len, 16).expect("Invalid layout");
            dealloc(self.ptr, layout)
        }
    }
}
