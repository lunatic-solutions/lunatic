use std::sync::Arc;

use anyhow::Result;
use wasmtime::ResourceLimiter;

use crate::{
    config::{ProcessConfig, UNIT_OF_COMPUTE_IN_INSTRUCTIONS},
    state::ProcessState,
    ExecutionResult, ResultValue,
};

use super::RawWasm;

#[derive(Clone)]
pub struct WasmtimeRuntime {
    engine: wasmtime::Engine,
}

impl WasmtimeRuntime {
    pub fn new(config: &wasmtime::Config) -> Result<Self> {
        let engine = wasmtime::Engine::new(config)?;
        Ok(Self { engine })
    }

    /// Compiles a wasm module to machine code and performs type-checking on host functions.
    pub fn compile_module<T>(&self, data: RawWasm) -> Result<WasmtimeCompiledModule<T>>
    where
        T: ProcessState,
    {
        let module = wasmtime::Module::new(&self.engine, data.as_slice())?;
        let mut linker = wasmtime::Linker::new(&self.engine);
        // Register host functions to linker.
        <T as ProcessState>::register(&mut linker)?;
        // The `default_state` and `store` are just used for resolving host functions that are not
        // owned by any particular `Store`. The "real" instance state and store are created inside
        // the `instantiate` function.
        // See: https://docs.rs/wasmtime/latest/wasmtime/struct.Linker.html#method.instantiate_pre
        // `default_state` should never be accessed and it's safe to use a "fake" state here.
        let default_state = T::state_for_instantiation();
        let mut store = wasmtime::Store::new(&self.engine, default_state);
        let instance_pre = linker.instantiate_pre(&mut store, &module)?;
        let compiled_module = WasmtimeCompiledModule::new(data, module, instance_pre);
        Ok(compiled_module)
    }

    pub async fn instantiate<T>(
        &self,
        compiled_module: &WasmtimeCompiledModule<T>,
        state: T,
    ) -> Result<WasmtimeInstance<T>>
    where
        T: ProcessState + Send + ResourceLimiter,
    {
        let max_fuel = state.config().get_max_fuel();
        let mut store = wasmtime::Store::new(&self.engine, state);
        // Set limits of the store
        store.limiter(|state| state);
        // Trap if out of fuel
        store.out_of_fuel_trap();
        // Define maximum fuel
        match max_fuel {
            Some(max_fuel) => {
                store.out_of_fuel_async_yield(max_fuel, UNIT_OF_COMPUTE_IN_INSTRUCTIONS)
            }
            // If no limit is specified use maximum
            None => store.out_of_fuel_async_yield(u64::MAX, UNIT_OF_COMPUTE_IN_INSTRUCTIONS),
        };
        // Create instance
        let instance = compiled_module
            .instantiator()
            .instantiate_async(&mut store)
            .await?;
        // Mark state as initialized
        store.data_mut().initialize();
        Ok(WasmtimeInstance { store, instance })
    }
}

pub struct WasmtimeCompiledModule<T> {
    inner: Arc<WasmtimeCompiledModuleInner<T>>,
}

pub struct WasmtimeCompiledModuleInner<T> {
    source: RawWasm,
    module: wasmtime::Module,
    instance_pre: wasmtime::InstancePre<T>,
}

impl<T> WasmtimeCompiledModule<T> {
    pub fn new(
        source: RawWasm,
        module: wasmtime::Module,
        instance_pre: wasmtime::InstancePre<T>,
    ) -> WasmtimeCompiledModule<T> {
        let inner = Arc::new(WasmtimeCompiledModuleInner {
            source,
            module,
            instance_pre,
        });
        Self { inner }
    }

    pub fn exports(&self) -> impl ExactSizeIterator<Item = wasmtime::ExportType<'_>> {
        self.inner.module.exports()
    }

    pub fn source(&self) -> &RawWasm {
        &self.inner.source
    }

    pub fn instantiator(&self) -> &wasmtime::InstancePre<T> {
        &self.inner.instance_pre
    }
}

impl<T> Clone for WasmtimeCompiledModule<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub struct WasmtimeInstance<T>
where
    T: Send,
{
    store: wasmtime::Store<T>,
    instance: wasmtime::Instance,
}

impl<T> WasmtimeInstance<T>
where
    T: Send,
{
    pub async fn call(mut self, function: &str, params: Vec<wasmtime::Val>) -> ExecutionResult<T> {
        let entry = self.instance.get_func(&mut self.store, function);

        if entry.is_none() {
            return ExecutionResult {
                state: self.store.into_data(),
                result: ResultValue::SpawnError(format!("Function '{}' not found", function)),
            };
        }

        let result = entry
            .unwrap()
            .call_async(&mut self.store, &params, &mut [])
            .await;

        ExecutionResult {
            state: self.store.into_data(),
            result: match result {
                Ok(()) => ResultValue::Ok,
                Err(err) => {
                    // If the trap is a result of calling `proc_exit(0)`, treat it as an no-error finish.
                    match err.downcast_ref::<wasmtime::Trap>() {
                        Some(trap) => ResultValue::Failed(trap.to_string()),
                        None => ResultValue::Failed(format!(
                            "Can't downcast trap ({}) to wasmtime::Trap",
                            err
                        )),
                    }
                }
            },
        }
    }
}

pub fn default_config() -> wasmtime::Config {
    let mut config = wasmtime::Config::new();
    config
        .async_support(true)
        .debug_info(false)
        // The behavior of fuel running out is defined on the Store
        .consume_fuel(true)
        .wasm_reference_types(true)
        .wasm_bulk_memory(true)
        .wasm_multi_value(true)
        .wasm_multi_memory(true)
        .cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize)
        // Allocate resources on demand because we can't predict how many process will exist
        .allocation_strategy(wasmtime::InstanceAllocationStrategy::OnDemand)
        // Always use static memories
        .static_memory_forced(true);
    config
}
