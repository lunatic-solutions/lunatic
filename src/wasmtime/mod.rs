//! This module contains a few helper structures and functions to work with Wasmtime.

use crate::channel;
use crate::process::{self, MemoryChoice, ProcessEnvironment};
use crate::wasi;
use wasmtime::{Config, Engine, Instance, Limits, Linker, Memory, MemoryType, Module, Store};

use anyhow::Result;

/// Contains data necessary to create Wasmtime instances suitable to be used with Lunatic processes.
/// Lunatic's instances have their own store, linker and process environment associated with them.
pub struct LunaticLinker {
    store: Store,
    module: Module,
    linker: Linker,
    proc_env: ProcessEnvironment,
}

impl LunaticLinker {
    /// Create a new LunaticLinker.
    pub fn new(
        engine: Engine,
        module: Module,
        yielder_ptr: usize,
        memory: MemoryChoice,
    ) -> Result<Self> {
        let store = Store::new(&engine);
        let mut linker = Linker::new(&store);

        let memory = match memory {
            MemoryChoice::Existing(memory) => memory,
            MemoryChoice::New(min_memory) => {
                let memory_ty = MemoryType::new(Limits::new(min_memory, None));
                Memory::new(&store, memory_ty)
            }
        };

        let environment =
            ProcessEnvironment::new(engine, module.clone(), memory.data_ptr(), yielder_ptr);

        linker.define("lunatic", "memory", memory)?;
        process::api::add_to_linker(&mut linker, environment.clone())?;
        channel::api::add_to_linker(&mut linker, environment.clone())?;
        wasi::api::add_to_linker(&mut linker, &environment)?;

        Ok(Self {
            store,
            linker,
            module,
            proc_env: environment,
        })
    }

    pub fn instance(&self) -> Result<Instance> {
        self.linker.instantiate(&self.module)
    }

    pub fn linker(&mut self) -> &mut Linker {
        &mut self.linker
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn proc_env(&mut self) -> &mut ProcessEnvironment {
        &mut self.proc_env
    }
}

/// Return a configured Wasmtime engine.
pub fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_threads(true);
    config.wasm_simd(true);
    config.wasm_reference_types(true);
    config.static_memory_guard_size(128 * 1024 * 1024); // 128 Mb
    Engine::new(&config)
}
