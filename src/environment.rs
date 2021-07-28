use anyhow::Result;
use lazy_static::lazy_static;
use tokio::task;
use wasmtime::{Config, Engine, InstanceAllocationStrategy, Linker, OptLevel, ProfilingStrategy};

use super::config::EnvConfig;
use crate::{api, module::Module, plugin::patch_module, state::ProcessState};

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
            engine,
            linker,
            config,
        })
    }

    /// Create a module from the environment.
    ///
    /// All plugins in this environment will get instantiated and their `lunatic_create_module_hook`
    /// function will be called. Plugins can use host functions to modify the module before it's JIT
    /// compiled by `Wasmtime`.
    pub async fn create_module(&self, data: Vec<u8>) -> Result<Module> {
        let env = self.clone();
        let new_module = patch_module(&data, self.config.plugins())?;
        // The compilation of a module is a CPU intensive tasks and can take some time.
        let module = task::spawn_blocking(move || {
            match wasmtime::Module::new(env.engine(), new_module.as_slice()) {
                Ok(wasmtime_module) => Ok(Module::new(data, env, wasmtime_module)),
                Err(err) => Err(err),
            }
        })
        .await??;
        Ok(module)
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn config(&self) -> &EnvConfig {
        &self.config
    }

    pub(crate) fn linker(&self) -> &Linker<ProcessState> {
        &self.linker
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
