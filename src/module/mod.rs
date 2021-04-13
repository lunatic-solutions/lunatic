use anyhow::Result;
#[cfg(feature = "vm-wasmer")]
use wasmer::Module as WasmerModule;
#[cfg(feature = "vm-wasmtime")]
use wasmtime::Module as WasmtimeModule;

use crate::linker::*;
use normalisation::patch;

pub mod normalisation;

#[derive(Debug, Clone, Copy)]
pub enum Runtime {
    #[cfg(feature = "vm-wasmtime")]
    Wasmtime,
    #[cfg(feature = "vm-wasmer")]
    Wasmer,
}

impl Default for Runtime {
    #[cfg(feature = "vm-wasmtime")]
    fn default() -> Self {
        Self::Wasmtime
    }

    #[cfg(not(feature = "vm-wasmtime"))]
    fn default() -> Self {
        Self::Wasmer
    }
}

#[derive(Clone)]
pub enum Module {
    #[cfg(feature = "vm-wasmtime")]
    Wasmtime(WasmtimeModule),
    #[cfg(feature = "vm-wasmer")]
    Wasmer(WasmerModule),
}

impl Module {
    #[cfg(feature = "vm-wasmtime")]
    pub fn wasmtime(&self) -> Option<&WasmtimeModule> {
        match self {
            Module::Wasmtime(m) => Some(m),
            #[cfg(feature = "vm-wasmer")]
            _ => None,
        }
    }

    #[cfg(feature = "vm-wasmer")]
    pub fn wasmer(&self) -> Option<&WasmerModule> {
        match self {
            Module::Wasmer(m) => Some(m),
            #[cfg(feature = "vm-wasmtime")]
            _ => None,
        }
    }

    pub fn runtime(&self) -> Runtime {
        match self {
            #[cfg(feature = "vm-wasmtime")]
            Module::Wasmtime(_) => Runtime::Wasmtime,
            #[cfg(feature = "vm-wasmer")]
            Module::Wasmer(_) => Runtime::Wasmer,
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
    pub fn new(wasm: &[u8], runtime: Runtime) -> Result<Self> {
        // Transfrom WASM file into a format compatible with Lunatic.
        let ((min_memory, max_memory), wasm) = patch(&wasm)?;

        let module = match runtime {
            #[cfg(feature = "vm-wasmtime")]
            Runtime::Wasmtime => Module::Wasmtime(WasmtimeModule::new(&wasmtime_engine(), wasm)?),
            #[cfg(feature = "vm-wasmer")]
            Runtime::Wasmer => Module::Wasmer(WasmerModule::new(&wasmer_engine(), wasm)?),
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
