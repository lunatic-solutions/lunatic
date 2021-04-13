use anyhow::Result;

use async_wormhole::AsyncYielder;
use uptown_funk::memory::Memory;

use std::mem::ManuallyDrop;
use std::{future::Future, marker::PhantomData};

use super::err::*;
use crate::module::Runtime;

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a mutable slice of Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe?
pub struct ProcessEnvironment<T: Clone> {
    memory: Memory,
    yielder: usize,
    yield_value: PhantomData<T>,
    runtime: Runtime,
}

impl<T: Sized + Clone> uptown_funk::Executor for ProcessEnvironment<T> {
    type Return = T;

    #[inline(always)]
    fn async_<R, F>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder = unsafe {
            std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<Result<T, Error<T>>>>)
        };
        yielder.async_suspend(f)
    }

    fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

// Because of a bug in Wasmtime: https://github.com/bytecodealliance/wasmtime/issues/2583
// we need to duplicate the Memory in the Linker before storing it in ProcessEnvironment,
// to not increase the reference count.
// When we are droping the memory we need to make sure we forget the value to not decrease
// the reference count.
// Safety: The ProcessEnvironment has the same lifetime as Memory, so it should be safe to
// do this.
impl<T: Sized + Clone> Drop for ProcessEnvironment<T> {
    fn drop(&mut self) {
        match self.runtime {
            #[cfg(feature = "vm-wasmtime")]
            Runtime::Wasmtime => {
                let memory = std::mem::replace(&mut self.memory, Memory::Empty);
                std::mem::forget(memory)
            }
            #[cfg(feature = "vm-wasmer")]
            _ => {}
        }
    }
}

// For the same reason mentioned on the Drop trait we can't increase the reference count
// on the Memory when cloning.
impl<T: Sized + Clone> Clone for ProcessEnvironment<T> {
    fn clone(&self) -> Self {
        match self.runtime {
            #[cfg(feature = "vm-wasmtime")]
            Runtime::Wasmtime => Self {
                memory: unsafe { std::ptr::read(&self.memory as *const Memory) },
                yielder: self.yielder,
                yield_value: PhantomData::default(),
                runtime: self.runtime,
            },
            #[cfg(feature = "vm-wasmer")]
            Runtime::Wasmer => Self {
                memory: self.memory.clone(),
                yielder: self.yielder,
                yield_value: PhantomData::default(),
                runtime: self.runtime,
            },
        }
    }
}

impl<T: Clone + Sized> ProcessEnvironment<T> {
    pub fn new(memory: Memory, yielder: usize, runtime: Runtime) -> Self {
        Self {
            memory,
            yielder,
            yield_value: PhantomData::default(),
            runtime,
        }
    }
}
