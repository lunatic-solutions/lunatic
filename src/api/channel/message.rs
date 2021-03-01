use std::{
    alloc::{alloc, dealloc, Layout},
    mem, ptr,
};

use super::host_resources::Resource;

pub struct Message {
    ptr: *mut u8,
    len: usize,
    pub host_resources: Vec<Resource>,
}

unsafe impl Send for Message {}

impl Message {
    pub fn new(source: *const u8, len: usize, host_resources: Vec<Resource>) -> Self {
        unsafe {
            let layout = Layout::from_size_align(len, 16).expect("Invalid layout");
            let ptr: *mut u8 = mem::transmute(alloc(layout));
            ptr::copy_nonoverlapping(source, ptr, len);
            Self {
                ptr,
                len,
                host_resources,
            }
        }
    }

    pub fn write_to(&self, destination: *mut u8) {
        unsafe {
            ptr::copy_nonoverlapping(self.ptr, destination, self.len);
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len, 16).expect("Invalid layout");
        unsafe { dealloc(self.ptr, layout) };
    }
}
