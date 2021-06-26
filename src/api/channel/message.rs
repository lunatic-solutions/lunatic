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
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn new(source: *const u8, len: usize, host_resources: Vec<Resource>) -> Self {
        let layout = Layout::from_size_align(len, 16).expect("Invalid layout");
        let ptr: *mut u8 = mem::transmute(alloc(layout));
        ptr::copy_nonoverlapping(source, ptr, len);
        Self {
            ptr,
            len,
            host_resources,
        }
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn write_to(&self, destination: *mut u8) {
        ptr::copy_nonoverlapping(self.ptr, destination, self.len);
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
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
