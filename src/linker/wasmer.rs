use crate::channel;
use crate::module::LunaticModule;
use crate::networking;
use crate::process::{self, MemoryChoice, ProcessEnvironment};
use crate::wasi;

use anyhow::Result;
use uptown_funk::{wasmer::WasmerLinker, HostFunctions};
use wasmer::{Exportable, Instance, Memory, MemoryType, Store};

/// Contains data necessary to create Wasmtime instances suitable to be used with Lunatic processes.
/// Lunatic's instances have their own store, linker and process environment associated with them.
pub struct LunaticLinker {
    linker: WasmerLinker,
    module: LunaticModule,
}

impl LunaticLinker {
    /// Create a new LunaticLinker.
    pub fn new(
        context_receiver: Option<channel::ChannelReceiver>,
        module: LunaticModule,
        yielder_ptr: usize,
        memory: MemoryChoice,
    ) -> Result<Self> {
        let store = engine();
        let mut linker = WasmerLinker::new();

        let memory = match memory {
            MemoryChoice::Existing => unimplemented!("No memory sharing yet"),
            MemoryChoice::New => {
                let memory_ty = MemoryType::new(module.min_memory(), module.max_memory(), false);
                Memory::new(&store, memory_ty).unwrap()
            }
        };

        let uptown_funk_memory: uptown_funk::memory::Memory = memory.clone().into();
        let environment = ProcessEnvironment::new(module.clone(), uptown_funk_memory, yielder_ptr);

        linker.add("lunatic", "memory", memory.to_export());

        let channel_state = channel::api::ChannelState::new(context_receiver);

        let process_state = process::api::ProcessState::new(module.clone(), channel_state.clone());
        process_state.add_to_wasmer_linker(environment.clone(), &mut linker, &store);

        let networking_state = networking::api::TcpState::new(channel_state.clone());
        networking_state.add_to_wasmer_linker(environment.clone(), &mut linker, &store);

        let wasi_state = wasi::api::WasiState::new();
        wasi_state.add_to_wasmer_linker(environment.clone(), &mut linker, &store);

        channel_state.add_to_wasmer_linker(environment, &mut linker, &store);

        Ok(Self { linker, module })
    }

    /// Create a new instance and set it up.
    /// This consumes the linker, as each of them is bound to one instance (environment).
    pub fn instance(self) -> Result<Instance> {
        let instance = Instance::new(self.module.module(), &self.linker).unwrap();
        Ok(instance)
    }
}

thread_local! {
    static STORE: Store = Store::default();
}

/// Return a configured Wasmer Store.
pub fn engine() -> Store {
    STORE.with(|store| store.clone())
}
