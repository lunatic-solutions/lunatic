use crate::module::LunaticModule;
use crate::process::{MemoryChoice, ProcessEnvironment};

use anyhow::Result;
use uptown_funk::{wasmer::WasmerLinker, HostFunctions};
use wasmer::{Exportable, Instance, Memory, MemoryType, Store};

/// Contains data necessary to create Wasmtime instances suitable to be used with Lunatic processes.
/// Lunatic's instances have their own store, linker and process environment associated with them.
pub struct LunaticLinker<T: HostFunctions> {
    linker: WasmerLinker,
    store: Store,
    module: LunaticModule,
    environment: ProcessEnvironment<T::Return>,
}

impl<T: HostFunctions> LunaticLinker<T> {
    /// Create a new LunaticLinker.
    pub fn new(module: LunaticModule, yielder_ptr: usize, memory: MemoryChoice) -> Result<Self> {
        let store = engine();
        let mut linker = WasmerLinker::new();

        let memory = match memory {
            MemoryChoice::Existing => unimplemented!("No memory sharing yet"),
            MemoryChoice::New => {
                let memory_ty = MemoryType::new(module.min_memory(), module.max_memory(), false);
                Memory::new(&store, memory_ty)?
            }
        };

        let uptown_funk_memory: uptown_funk::memory::Memory = memory.clone().into();
        let environment = ProcessEnvironment::new(uptown_funk_memory, yielder_ptr);

        linker.add("lunatic", "memory", memory.to_export());

        Ok(Self {
            linker,
            store,
            module,
            environment,
        })
    }

    /// Create a new instance and set it up.
    /// This consumes the linker, as each of them is bound to one instance (environment).
    pub fn instance(self) -> Result<Instance> {
        let instance = Instance::new(self.module.module(), &self.linker)?;
        Ok(instance)
    }

    pub fn add_api(&mut self, state: T) -> T::Return {
        state.add_to_wasmer_linker(self.environment.clone(), &mut self.linker, &self.store)
    }
}

thread_local! {
    static STORE: Store = Store::default();
}

/// Return a configured Wasmer Store.
pub fn engine() -> Store {
    STORE.with(|store| store.clone())
}
