use crate::channel;
use crate::module::LunaticModule;
use crate::networking;
use crate::process::{self, MemoryChoice, ProcessEnvironment};
use crate::wasi;

use anyhow::Result;
use std::sync::Once;
use uptown_funk::HostFunctions;
use wasmtime::{Config, Engine, Instance, Limits, Linker, Memory, MemoryType, Store};

/// Contains data necessary to create Wasmtime instances suitable to be used with Lunatic processes.
/// Lunatic's instances have their own store, linker and process environment associated with them.
pub struct LunaticLinker {
    linker: Linker,
    module: LunaticModule,
}

impl LunaticLinker {
    /// Create a new LunaticLinker.
    pub fn new(module: LunaticModule, yielder_ptr: usize, memory: MemoryChoice) -> Result<Self> {
        let engine = engine();
        let store = Store::new(&engine);
        let mut linker = Linker::new(&store);

        let memory = match memory {
            MemoryChoice::Existing => unimplemented!("No memory sharing yet"),
            MemoryChoice::New => {
                let memory_ty =
                    MemoryType::new(Limits::new(module.min_memory(), module.max_memory()));
                Memory::new(&store, memory_ty)
            }
        };

        let uptown_funk_memory: uptown_funk::memory::Memory = memory.clone().into();
        let environment = ProcessEnvironment::new(module.clone(), uptown_funk_memory, yielder_ptr);

        linker.define("lunatic", "memory", memory)?;

        let process_state = process::api::ProcessState::new(module.clone());
        process_state.add_to_linker(environment.clone(), &mut linker);

        let channel_state = channel::api::ChannelState::new();
        channel_state.add_to_linker(environment.clone(), &mut linker);

        let networking_state = networking::api::TcpState::new();
        networking_state.add_to_linker(environment.clone(), &mut linker);

        let wasi_state = wasi::api::WasiState::new();
        wasi_state.add_to_linker(environment, &mut linker);

        Ok(Self { linker, module })
    }

    /// Create a new instance and set it up.
    /// This consumes the linker, as each of them is bound to one instance (environment).
    pub fn instance(self) -> Result<Instance> {
        let instance = self.linker.instantiate(self.module.module())?;
        Ok(instance)
    }
}

/// Return a configured Wasmtime engine.
pub fn engine() -> Engine {
    static mut ENGINE: Option<Engine> = None;
    static INIT: Once = Once::new();
    unsafe {
        INIT.call_once(|| {
            let mut config = Config::new();
            config.wasm_threads(true);
            config.wasm_simd(true);
            config.wasm_reference_types(true);
            config.static_memory_guard_size(8 * 1024 * 1024); // 8 Mb
            ENGINE = Some(Engine::new(&config));
        });
        ENGINE.clone().unwrap()
    }
}
