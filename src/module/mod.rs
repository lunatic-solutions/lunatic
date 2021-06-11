use anyhow::Result;
use wasmtime::Module as WasmtimeModule;

use crate::linker::*;
use normalisation::patch;

pub mod normalisation;

#[derive(Debug, Clone, Copy)]
pub enum Runtime {
    Wasmtime,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::Wasmtime
    }
}

#[derive(Clone)]
pub enum Module {
    Wasmtime(WasmtimeModule),
}

impl Module {
    pub fn wasmtime(&self) -> Option<&WasmtimeModule> {
        match self {
            Module::Wasmtime(m) => Some(m),
        }
    }

    pub fn runtime(&self) -> Runtime {
        match self {
            Module::Wasmtime(_) => Runtime::Wasmtime,
        }
    }
}

#[derive(Clone)]
pub struct LunaticModule {
    module: Module,
    min_memory: u32,
    max_memory: Option<u32>,
}

impl LunaticModule {
    pub fn new(
        wasm: &[u8],
        runtime: Runtime,
        is_profile: bool,
        is_normalisation_out: bool,
    ) -> Result<Self> {
        // Transform WASM file into a format compatible with Lunatic.
        let ((min_memory, max_memory), wasm) = patch(&wasm, is_profile, is_normalisation_out)?;

        let module = match runtime {
            Runtime::Wasmtime => Module::Wasmtime(WasmtimeModule::new(&wasmtime_engine(), wasm)?),
        };

        Ok(Self {
            module,
            min_memory,
            max_memory,
        })
    }

    pub fn runtime(&self) -> Runtime {
        self.module.runtime()
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn min_memory(&self) -> u32 {
        self.min_memory
    }

    pub fn max_memory(&self) -> Option<u32> {
        self.max_memory
    }
}
