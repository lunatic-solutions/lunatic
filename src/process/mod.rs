pub mod channel;
pub mod creator;
pub mod imports;
pub mod pool;

use async_wormhole::AsyncYielder;
use wasmtime::{Engine, Memory, Module};

use std::future::Future;
use std::mem::ManuallyDrop;
use thiserror::Error;

#[derive(Clone)]
pub struct ProcessEnvironment {
    engine: Engine,
    module: Module,
    memory: Memory,
    yielder: usize,
}

impl ProcessEnvironment {
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        let mut yielder =
            unsafe { std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<()>>) };
        yielder.async_suspend(future)
    }

    pub fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("instantation error")]
    Instantiation(#[from] wasmer::InstantiationError),
    #[error("heap allocation error")]
    HeapAllocation(#[from] wasmer::MemoryError),
    #[error("runtime error")]
    Runtime(#[from] wasmer::RuntimeError),
}
