use async_wormhole::AsyncYielder;
use uptown_funk::memory::Memory;

use std::future::Future;
use std::mem::ManuallyDrop;

use crate::module::Runtime;

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a mutable slice of Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe?
pub struct ProcessEnvironment {
    memory: Memory,
    yielder: usize,
    runtime: Runtime,
}

impl uptown_funk::Executor for ProcessEnvironment {
    #[inline(always)]
    fn async_<R, F>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder = unsafe {
            std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<anyhow::Result<()>>>)
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
// When we are dropping the memory we need to make sure we forget the value to not decrease
// the reference count.
// Safety: The ProcessEnvironment has the same lifetime as Memory, so it should be safe to
// do this.
impl Drop for ProcessEnvironment {
    fn drop(&mut self) {
        match self.runtime {
            Runtime::Wasmtime => {
                let memory = std::mem::replace(&mut self.memory, Memory::Empty);
                std::mem::forget(memory)
            }
        }
    }
}

// For the same reason mentioned on the Drop trait we can't increase the reference count
// on the Memory when cloning.
impl Clone for ProcessEnvironment {
    fn clone(&self) -> Self {
        match self.runtime {
            Runtime::Wasmtime => Self {
                memory: unsafe { std::ptr::read(&self.memory as *const Memory) },
                yielder: self.yielder,
                runtime: self.runtime,
            },
        }
    }
}

impl ProcessEnvironment {
    pub fn new(memory: Memory, yielder: usize, runtime: Runtime) -> Self {
        Self {
            memory,
            runtime,
            yielder,
        }
    }
}
