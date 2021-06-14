use crate::{
    api::channel::{api::ChannelState, ChannelReceiver, Message},
    api::heap_profiler,
    module::{LunaticModule, Runtime},
};

use super::module_linking::{Import, Imports, ModuleResult};
use super::{FunctionLookup, MemoryChoice, Process};

use anyhow::Result;
use log::error;
use smol::{channel::bounded, future::yield_now, Timer};
use uptown_funk::{host_functions, state::HashMapStore, HostFunctions};

use std::{
    mem::replace,
    time::{Duration, Instant},
};

pub struct ProcessState {
    module: LunaticModule,
    channel_state: ChannelState,
    pub processes: HashMapStore<Process>,
    pub modules: HashMapStore<LunaticModule>,
    pub imports: HashMapStore<Import>,
    profiler: <heap_profiler::HeapProfilerState as HostFunctions>::Wrap,
}

impl ProcessState {
    pub fn new(
        module: LunaticModule,
        channel_state: ChannelState,
        profiler: <heap_profiler::HeapProfilerState as HostFunctions>::Wrap,
    ) -> Self {
        Self {
            module,
            channel_state,
            processes: HashMapStore::new(),
            modules: HashMapStore::new(),
            imports: HashMapStore::new(),
            profiler,
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl ProcessState {
    // Yield this process allowing other to be scheduled on same thread.
    async fn yield_(&self) {
        yield_now().await
    }

    // Suspend process for `millis`.
    async fn sleep_ms(&self, millis: u64) {
        let now = Instant::now();
        let when = now + Duration::from_millis(millis);
        Timer::at(when).await;
    }

    // Spawn a new process with a context and call a function from the function table by `index`.
    //
    // Once the process is created the context will be passed through a Channel::Receiver to it.
    async fn spawn_with_context(&self, index: u32, context: &[u8]) -> Process {
        let (sender, receiver) = bounded(1);
        let host_resources = &mut self
            .channel_state
            .inner
            .borrow_mut()
            .next_message_host_resources;
        let host_resources = replace(host_resources, Vec::new());
        let message = Message::new(context.as_ptr(), context.len(), host_resources);
        let _ignore = sender.send(message).await;

        let future = Process::create(
            Some(ChannelReceiver::from(receiver)),
            self.module.clone(),
            FunctionLookup::TableIndex(index),
            MemoryChoice::New(None),
            self.profiler.clone(),
        );
        Process::spawn(future)
    }

    // Wait on child process to finish.
    // Returns 0 if process didn't trap, otherwise 1
    async fn join(&self, process: Process) -> u32 {
        match process.task().await {
            Ok(()) => 0,
            Err(err) => {
                error!(target: "process", "Process trapped: {}", err);
                1
            }
        }
    }

    // Drops the Task and cancels the process
    async fn cancel_process(&self, _process: Process) {
        // _process will take ownership here of the underlying task and drop it.
        // See: https://docs.rs/smol/latest/smol/struct.Task.html
    }

    // Detaches process
    async fn detach_process(&self, process: Process) {
        process.task().detach()
    }

    // Load a WASM buffer and create a module from it.
    fn load_module(&self, wasm: &[u8]) -> (u32, ModuleResult) {
        match LunaticModule::new(wasm, Runtime::default(), false, false) {
            Ok(module) => (0, ModuleResult::Ok(module)),
            Err(err) => {
                error!(target: "process", "Module load error: {}", err);
                (1, ModuleResult::Err)
            }
        }
    }

    fn unload_module(&mut self, id: u32) {
        self.modules.remove(id);
    }

    // Define import from module
    fn create_import(&self, name: &str, module: LunaticModule) -> Import {
        Import(name.to_string(), module)
    }

    fn remove_import(&mut self, id: u32) {
        self.imports.remove(id);
    }

    // Spawn a process from a module.
    //
    // imports - buffer of u32 indexes of imports used to satisfy instantiation.
    fn spawn_from_module(
        &self,
        module: LunaticModule,
        name: &str,
        max_memory: u32,
        imports: &[u8],
    ) -> Process {
        let imports: Vec<Option<Import>> = imports
            .windows(4)
            .map(|import| {
                let mut import_exact = [0; 4];
                import_exact.copy_from_slice(import); // Safe because window size is 4
                let import_id = u32::from_le_bytes(import_exact);
                self.imports.get(import_id).map(|x| x.clone())
            })
            .collect();
        let imports = Imports::new(module.clone(), imports);
        let (_, future) = Process::create_with_api(
            module,
            FunctionLookup::Name(String::from(name)),
            MemoryChoice::New(Some(max_memory)),
            imports,
        )
        .unwrap();
        Process::spawn(future)
    }
}
