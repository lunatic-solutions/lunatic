use crossbeam::queue::{ArrayQueue, PopError};
use wasmtime::{Engine, Instance, Limits, Linker, Memory, MemoryType, Module, Store};

use super::imports::create_lunatic_imports;
use super::ProcessEnvironment;
use crate::wasi::create_wasi_imports;

/// Each process uses a lot of virtual memory (6 Gb). Even vritual memory is cheap we need to have
/// a hard cap at around 20k processes or we risk to run out of virtual memory on a 64bit system.
pub const PROCESS_CAPACITY: usize = 20_000;

#[allow(dead_code)]
pub struct StoreLinkerEnvironment {
    store: Store,
    linker: Linker,
    environment: ProcessEnvironment,
}

impl StoreLinkerEnvironment {
    pub fn instantiate(&self, module: &Module) -> Instance {
        self.linker.instantiate(module).unwrap()
    }
}

/// Wasmtime's store holds references to all instances, linkers, memories and tables created in it.
/// This resources will only be freed once the Store is gone.
pub struct StoreLinkerPool {
    pool: ArrayQueue<StoreLinkerEnvironment>,
}

unsafe impl Sync for StoreLinkerPool {}

impl StoreLinkerPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            pool: ArrayQueue::new(capacity),
        }
    }

    pub fn get(
        &self,
        engine: Engine,
        module: Module,
        memory: Option<Memory>,
        yielder: usize,
    ) -> StoreLinkerEnvironment {
        match self.pool.pop() {
            Err(PopError) => {
                let store = Store::new(&engine);
                let mut linker = Linker::new(&store);

                let memory = match memory {
                    Some(memory) => memory,
                    None => {
                        let memory_ty = MemoryType::new(Limits::new(17, None));
                        Memory::new(&store, memory_ty)
                    }
                };

                let environment = ProcessEnvironment {
                    engine,
                    module,
                    memory,
                    yielder,
                };

                create_lunatic_imports(&mut linker, environment.clone());
                create_wasi_imports(&mut linker, environment.clone());

                StoreLinkerEnvironment {
                    store,
                    linker,
                    environment,
                }
            }
            Ok(mut store_linker_pool) => {
                store_linker_pool.environment.yielder = yielder;
                store_linker_pool
            }
        }
    }

    pub fn recycle(&self, store_linker_env: StoreLinkerEnvironment) {
        // If we push over the capacity just drop the StoreLinkerEnvironment.
        let _ = self.pool.push(store_linker_env);
    }
}
