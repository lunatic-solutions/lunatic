use std::sync::Arc;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use tokio::{sync::mpsc::unbounded_channel, task};
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, Linker, Module, OptLevel, ProfilingStrategy, Store,
};

use super::config::EnvConfig;
use crate::{
    api, message::Message, plugin::patch_module, process::ProcessHandle, state::ProcessState,
};

// One gallon of fuel represents around 100k instructions.
pub const GALLON_IN_INSTRUCTIONS: u64 = 100_000;

/// The environment represents a set of characteristics that processes spawned from it will have.
///
/// Environments let us set limits on processes:
/// * Memory limits
/// * Compute limits
/// * Access to host functions
///
/// They also define the set of plugins. Plugins can be used to modify loaded Wasm modules or to
/// limit processes' access to host functions by shadowing their implementations.
#[derive(Clone)]
pub struct Environment {
    inner: Arc<InnerEnv>,
}

struct InnerEnv {
    engine: Engine,
    linker: Linker<ProcessState>,
    config: EnvConfig,
}

impl Environment {
    /// Create a new environment from a configuration.
    pub fn new(config: EnvConfig) -> Result<Self> {
        let mut wasmtime_config = Config::new();
        wasmtime_config
            .async_support(true)
            .debug_info(false)
            // The behaviour of fuel running out is defined on the Store
            .consume_fuel(true)
            .wasm_reference_types(true)
            .wasm_bulk_memory(true)
            .wasm_multi_value(true)
            .wasm_multi_memory(true)
            .wasm_module_linking(true)
            // Disable profiler
            .profiler(ProfilingStrategy::None)?
            .cranelift_opt_level(OptLevel::SpeedAndSize)
            // Allocate resources on demand because we can't predict how many process will exist
            .allocation_strategy(InstanceAllocationStrategy::OnDemand)
            // Memories are always static (can't be bigger than max_memory)
            .static_memory_maximum_size(config.max_memory())
            // Set memory guards to 4 Mb
            .static_memory_guard_size(0x400000)
            .dynamic_memory_guard_size(0x400000);
        let engine = Engine::new(&wasmtime_config)?;
        let mut linker = Linker::new(&engine);
        // Allow plugins to shadow host functions
        linker.allow_shadowing(true);

        // Register host functions for linker
        api::register(&mut linker, config.allowed_namespace())?;

        Ok(Self {
            inner: Arc::new(InnerEnv {
                engine,
                linker,
                config,
            }),
        })
    }

    /// Spawns a new process from the environment.
    ///
    /// A `Process` is created from a `Module` and an entry `function`. The configuration of the
    /// environment will define some characteristics of the process, such as maximum memory, fuel
    /// and available host functions.
    ///
    /// After it's spawned the process will keep running in the background. A process can be killed
    /// by sending a `Signal::Kill` to it.
    pub async fn spawn(&self, module: &Module, function: &str) -> Result<ProcessHandle> {
        let (mailbox_sender, mailbox) = unbounded_channel::<Message>();
        let state = ProcessState::new(self.clone(), mailbox);
        let mut store = Store::new(&self.inner.engine, state);
        store.limiter(|state| state);

        // Trap if out of fuel
        store.out_of_fuel_trap();
        // Define maximum fuel
        match self.inner.config.max_fuel() {
            Some(max_fuel) => store.out_of_fuel_async_yield(max_fuel, GALLON_IN_INSTRUCTIONS),
            // If no limit is specified use maximum
            None => store.out_of_fuel_async_yield(u64::MAX, GALLON_IN_INSTRUCTIONS),
        };

        let instance = self
            .inner
            .linker
            .instantiate_async(&mut store, &module)
            .await?;
        let entry = instance
            .get_func(&mut store, &function)
            .map_or(Err(anyhow!("Function '{}' not found", function)), |func| {
                Ok(func)
            })?;

        let fut = async move { entry.call_async(&mut store, &[]).await };
        Ok(ProcessHandle::new(fut, mailbox_sender))
    }

    /// Create a module from the environment.
    ///
    /// All plugins in this environment will get instantiated and their `lunatic_create_module_hook`
    /// function will be called. Plugins can use host functions to modify the module before it's JIT
    /// compiled by `Wasmtime`.
    pub async fn create_module(&self, module: Vec<u8>) -> Result<Module> {
        let engine = self.inner.engine.clone();
        let new_module = patch_module(&module, self.inner.config.plugins())?;
        // The compilation of a module is a CPU intensive tasks and can take some time.
        let module =
            task::spawn_blocking(move || Module::new(&engine, new_module.as_slice())).await??;
        Ok(module)
    }

    // Returns the max memory allowed by this environment
    pub fn max_memory(&self) -> u64 {
        self.inner.config.max_memory()
    }

    pub fn engine(&self) -> &Engine {
        &self.inner.engine
    }
}

// All plugins share one environment
pub(crate) struct PluginEnv {
    pub(crate) engine: Engine,
}

lazy_static! {
    pub(crate) static ref PLUGIN_ENV: PluginEnv = {
        let engine = Engine::default();
        PluginEnv { engine }
    };
}
