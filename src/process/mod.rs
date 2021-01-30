pub mod api;
mod tls;

use anyhow::Result;
use async_wormhole::{
    stack::{OneMbStack, Stack},
    AsyncWormhole, AsyncYielder,
};
use lazy_static::lazy_static;
use smol::{Executor as TaskExecutor, Task};
use uptown_funk::{memory::Memory, Executor, FromWasm, HostFunctions, ToWasm};

use crate::module::LunaticModule;
use crate::{channel::ChannelReceiver, linker::LunaticLinker};

use crate::channel;
use crate::networking;
use crate::process;
use crate::wasi;

use log::info;
use std::future::Future;
use std::mem::ManuallyDrop;

lazy_static! {
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
    memory: Memory,
    yielder: usize,
}

impl uptown_funk::Executor for ProcessEnvironment {
    #[inline(always)]
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
            memory: unsafe { std::ptr::read(&self.memory as *const Memory) },
            yielder: self.yielder,
        }
    }
}

impl ProcessEnvironment {
    pub fn new(memory: Memory, yielder: usize) -> Self {
        Self { memory, yielder }
    }
}

/// A lunatic process represents an actor.
pub struct Process {
    task: Task<Result<()>>,
}

impl Process {
    pub fn task(self) -> Task<Result<()>> {
        self.task
    }

    /// Spawns a new process with a custom API.
    pub fn spawn_with_api<A>(
        module: LunaticModule,
        function: FunctionLookup,
        memory: MemoryChoice,
        api: A,
    ) -> Self
    where
        A: HostFunctions + 'static,
    {
        let stack = OneMbStack::new().unwrap();
        let process = AsyncWormhole::new(stack, move |yielder| {
            let yielder_ptr = &yielder as *const AsyncYielderCast as usize;

            let mut linker = LunaticLinker::new(module, yielder_ptr, memory)?;
            linker.add_api(api);
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
                    let func = instance
                        .exports
                        .get_function("lunatic_spawn_by_index")
                        .unwrap();
                    #[cfg(feature = "vm-wasmtime")]
                    let func = instance.get_func("lunatic_spawn_by_index").unwrap();

                    func.call(&[(index as i32).into()])?;
                }
            }

            Ok(())
        });

        let task = EXECUTOR.spawn(async {
            let mut process = process?;

            let ctss = tls::CallThreadStateSave::new();
            process.set_pre_post_poll(move || ctss.swap());

            let result = (&mut process).await.unwrap();
            result
        });

        Self { task }
    }

    /// Spawn a new process using the default api.
    pub fn spawn(
        context_receiver: Option<ChannelReceiver>,
        module: LunaticModule,
        function: FunctionLookup,
        memory: MemoryChoice,
    ) -> Self {
        let api = DefaultApi::new(context_receiver, module.clone());
        Process::spawn_with_api(module, function, memory, api)
    }
}

pub struct DefaultApi {
    context_receiver: Option<ChannelReceiver>,
    module: LunaticModule,
}

impl DefaultApi {
    pub fn new(context_receiver: Option<ChannelReceiver>, module: LunaticModule) -> Self {
        Self {
            context_receiver,
            module,
        }
    }
}

impl HostFunctions for DefaultApi {
    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(self.context_receiver);
        let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        let networking_state = networking::api::TcpState::new(channel_state.clone());
        let wasi_state = wasi::api::WasiState::new();

        channel_state.add_to_linker(executor.clone(), linker);
        process_state.add_to_linker(executor.clone(), linker);
        networking_state.add_to_linker(executor.clone(), linker);
        wasi_state.add_to_linker(executor, linker);
    }

    #[cfg(feature = "vm-wasmer")]
    fn add_to_wasmer_linker<E>(
        self,
        executor: E,
        linker: &mut uptown_funk::wasmer::WasmerLinker,
        store: &wasmer::Store,
    ) where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(self.context_receiver);
        let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        let networking_state = networking::api::TcpState::new(channel_state.clone());
        let wasi_state = wasi::api::WasiState::new();

        channel_state.add_to_wasmer_linker(executor.clone(), linker, store);
        process_state.add_to_wasmer_linker(executor.clone(), linker, store);
        networking_state.add_to_wasmer_linker(executor.clone(), linker, store);
        wasi_state.add_to_wasmer_linker(executor, linker, store);
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
