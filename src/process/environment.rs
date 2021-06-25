use anyhow::{anyhow, Result};
use tokio::task;
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, Limits, Linker, Memory, MemoryType, Module,
    OptLevel, ProfilingStrategy, Store, Val,
};

pub use super::config::EnvConfig;
use crate::{api, state::State};

/// One gallon of fuel represents around 10k instructions.
const GALLON_IN_INSTRUCTIONS: u64 = 10_000;

/// The environment represents a set of characteristics that processes spawned from it will have.
///
/// Environments let us set limits on processes:
/// * Memory limits
/// * Compute limits
///
/// They also define the set of plugins that are used by the process. Plugins then can be used to
/// limit processes' access to host functions.
pub struct Environment {
    engine: Engine,
    linker: Linker<State>,
    config: EnvConfig,
    plugins: Vec<(String, Module)>,
}

impl Environment {
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
            plugins: Vec::new(),
        })
    }

    /// Adds module as a plugin that will be used on all processes in this environment
    pub fn add_plugin<S: Into<String>>(&mut self, namespace: S, module: Vec<u8>) -> Result<()> {
        let module = Module::new(&self.engine, module)?;
        self.plugins.push((namespace.into(), module));
        Ok(())
    }

    /// Removes last plugin
    pub fn remove_last_plugin(&mut self) {
        self.plugins.pop();
    }

    pub async fn spawn(&self, module: &Module, function: &str) -> Result<()> {
        let state = State::default();
        let mut store = Store::new(&self.engine, state);

        // Trap if out of fuel
        store.out_of_fuel_trap();
        // Define maximum fuel
        match self.config.max_fuel() {
            Some(max_fuel) => store.out_of_fuel_async_yield(max_fuel, GALLON_IN_INSTRUCTIONS),
            // If no limit is specified use maximum
            // TODO: Could we run out of u32::MAX * 10k instructions? (3 hours on a 4Ghz CPU?)
            None => store.out_of_fuel_async_yield(u32::MAX, GALLON_IN_INSTRUCTIONS),
        };

        // Don't poison the common linker with a particular `Store`
        let mut linker = self.linker.clone();

        // Add plugins to host functions
        for (namespace, module) in self.plugins.iter() {
            let instance = linker.instantiate_async(&mut store, module).await?;
            // Call the initialize method on the plugin if it exists
            if let Some(initialize) = instance.get_func(&mut store, "_initialize") {
                initialize.call_async(&mut store, &[]).await?;
            }
            linker.instance(&mut store, namespace, instance)?;
        }

        // Create memory for module
        let memory_ty = MemoryType::new(Limits::new(1, None));
        let memory = Memory::new(&mut store, memory_ty)?;

        let instance = linker.instantiate_async(&mut store, &module).await?;
        let entry = instance
            .get_func(&mut store, &function)
            .map_or(Err(anyhow!("Function '{}' not found", function)), |func| {
                Ok(func)
            })?;

        entry.call_async(&mut store, &[]).await?;

        Ok(())
    }

    pub async fn create_module(&self, data: Vec<u8>) -> Result<Module> {
        let module_size = data.len();
        let mut state = State::default();
        state.module_loaded = Some(data);

        let mut store = Store::new(&self.engine, state);
        // Give enough fuel for plugins to run.
        store.out_of_fuel_async_yield(u32::MAX, GALLON_IN_INSTRUCTIONS);
        // Run plugin hooks on the .wasm file
        for (_, module) in self.plugins.iter() {
            let instance = self.linker.instantiate_async(&mut store, module).await?;
            // Call the initialize method on the plugin if it exists
            if let Some(initialize) = instance.get_func(&mut store, "_initialize") {
                initialize.call_async(&mut store, &[]).await?;
            }
            match instance.get_func(&mut store, "lunatic_create_module_hook") {
                Some(hook) => {
                    hook.call_async(&mut store, &[Val::I64(module_size as i64)])
                        .await?;
                }
                None => (),
            };
        }

        let new_data = match store.data_mut().module_loaded.take() {
            Some(module) => module,
            None => return Err(anyhow!("create_module failed: Module doesn't exist")),
        };

        let engine = self.engine.clone();
        // The compilation of a module is a CPU intensive tasks and can take some time.
        let module =
            task::spawn_blocking(move || Module::new(&engine, new_data.as_slice())).await??;
        Ok(module)
    }
}
