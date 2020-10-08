pub mod channel;
pub mod creator;
pub mod imports;
pub mod pool;

use async_wormhole::AsyncYielder;
use wasmtime::{Engine, Module, Memory};

use std::future::Future;
use std::rc::Rc;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use thiserror::Error;

#[derive(Clone)]
pub struct ProcessEnvironment {
    inner: Rc<RefCell<ProcessEnvironmentInner>>
}

struct ProcessEnvironmentInner {
    engine: Engine,
    module: Module,
    // memory: Memory,
    yielder: usize,
}

impl ProcessEnvironment {
    pub fn new(engine: Engine, module: Module, yielder: usize) -> Self {
        ProcessEnvironment {
            inner: Rc::new(RefCell::new(ProcessEnvironmentInner { engine, module, yielder } ))
        }
    }

    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        let mut yielder =
            unsafe { std::ptr::read(self.inner.borrow().yielder as *const ManuallyDrop<AsyncYielder<R>>) };
        yielder.async_suspend(future)
    }

    // pub fn memory(&self) -> Memory {
    //     self.inner.borrow().memory.clone()
    // }

    // pub fn set_memory(&self, memory: Memory) {
    //     self.inner.borrow_mut().memory = memory;
    // }

    pub fn engine(&self) -> Engine {
        self.inner.borrow().engine.clone()
    }

    pub fn module(&self) -> Module {
        self.inner.borrow().module.clone()
    }

    pub fn set_yielder(&mut self, yielder: usize) {
        self.inner.borrow_mut().yielder = yielder;
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
