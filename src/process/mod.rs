pub mod creator;

use wasmer::{Instance, Memory};
use async_wormhole::AsyncYielder;
use std::mem::ManuallyDrop;
use std::future::Future;

#[derive(Clone)]
pub struct Process {
    instance: Instance,
    memory: Memory,
    yielder: usize
}

impl Process {
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        let mut yielder = unsafe {
            std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<()>>)
        };
        yielder.async_suspend( future )
    }
}