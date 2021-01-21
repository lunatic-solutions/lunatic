pub mod api;

use anyhow::Result;
use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;
use lazy_static::lazy_static;
use smol::{Executor as TaskExecutor, Task};
use uptown_funk::{memory::Memory, Executor, FromWasm, ToWasm};

use crate::module::LunaticModule;
use crate::{channel::ChannelReceiver, linker::LunaticLinker};

use log::info;
use std::future::Future;
use std::mem::ManuallyDrop;

lazy_static! {
    static ref WORMHOLE_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: TaskExecutor<'static> = TaskExecutor::new();
}

pub type AsyncYielderCast<'a> = AsyncYielder<'a, Result<()>>;

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    TableIndex(u32),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances.
#[derive(Clone)]
pub enum MemoryChoice {
    Existing,
    New,
}

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a mutable slice of Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe?
#[cfg_attr(feature = "vm-wasmer", derive(Clone))]
pub struct ProcessEnvironment {
    module: LunaticModule,
    memory: Memory,
    yielder: usize,
}

impl uptown_funk::Executor for ProcessEnvironment {
    fn async_<R, F>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder =
            unsafe { std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielderCast>) };
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
#[cfg(feature = "vm-wasmtime")]
impl Drop for ProcessEnvironment {
    fn drop(&mut self) {
        let memory = std::mem::replace(&mut self.memory, Memory::Empty);
        std::mem::forget(memory)
    }
}

// For the same reason mentioned on the Drop trait we can't increase the reference count
// on the Memory when cloning.
#[cfg(feature = "vm-wasmtime")]
impl Clone for ProcessEnvironment {
    fn clone(&self) -> Self {
        Self {
            module: self.module.clone(),
            memory: unsafe { std::ptr::read(&self.memory as *const Memory) },
            yielder: self.yielder,
        }
    }
}

impl ProcessEnvironment {
    pub fn new(module: LunaticModule, memory: Memory, yielder: usize) -> Self {
        Self {
            module,
            memory,
            yielder,
        }
    }
}

/// A lunatic process represents an actor.
pub struct Process {
    task: Task<Result<()>>,
}

impl Process {
    pub fn join(self) -> Task<Result<()>> {
        self.task
    }

    /// Spawn a new process.
    pub fn spawn(
        context_receiver: Option<ChannelReceiver>,
        module: LunaticModule,
        function: FunctionLookup,
        memory: MemoryChoice,
    ) -> Self {
        #[cfg(feature = "vm-wasmer")]
        let tls = [&wasmer_vm::traphandlers::tls::PTR];
        #[cfg(feature = "vm-wasmtime")]
        let tls = [&wasmtime_runtime::traphandlers::tls::PTR];
        let process = WORMHOLE_POOL.with_tls(
            tls,
            move |yielder| {
                let yielder_ptr = &yielder as *const AsyncYielderCast as usize;

                let linker = LunaticLinker::new(context_receiver, module, yielder_ptr, memory)?;
                let instance = linker.instance()?;

                match function {
                    FunctionLookup::Name(name) => {
                        #[cfg(feature = "vm-wasmer")]
                        let func = instance.exports.get_function(name).unwrap();
                        #[cfg(feature = "vm-wasmtime")]
                        let func = instance.get_func(name).unwrap();

                        // Measure how long the function takes for named functions.
                        let performance_timer = std::time::Instant::now();
                        func.call(&[])?;
                        info!(target: "performance", "Process {} finished in {} ms.", name, performance_timer.elapsed().as_millis());
                    }
                    FunctionLookup::TableIndex(index) => {
                        #[cfg(feature = "vm-wasmer")]
                        let func = instance.exports.get_function("lunatic_spawn_by_index").unwrap();
                        #[cfg(feature = "vm-wasmtime")]
                        let func = instance.get_func("lunatic_spawn_by_index").unwrap();

                        func.call(&[(index as i32).into()])?;
                    }
                }

                Ok(())
            },
        );

        let task = EXECUTOR.spawn(async {
            let mut process = process?;
            let result = (&mut process).await.unwrap();
            WORMHOLE_POOL.recycle(process);
            result
        });

        Self { task }
    }
}

impl ToWasm for Process {
    type To = u32;
    type State = api::ProcessState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        process: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.processes.add(process))
    }
}

impl FromWasm for Process {
    type From = u32;
    type State = api::ProcessState;

    fn from(
        state: &mut Self::State,
        _: &impl Executor,
        process_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.processes.remove(process_id) {
            Some(process) => Ok(process),
            None => Err(uptown_funk::Trap::new("Process not found")),
        }
    }
}
