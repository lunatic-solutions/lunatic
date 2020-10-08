//! Wasmtime resources like instances, memories, tables, etc. are bound to a store and not freed
//! until the store is dropped. But creating one store and linker per instance is costy, so here
//! we attempt to pool stores and linkers together.
//! We just need to take care not to reuse a store too many times or it will end up holding onto
//! resources for a long time. We are mostly concerned about **memories**. Each memory takes up
//! around 6 Gb of virtual memory. Even virtual memory is cheap on 64bit systems if we have around
//! 20k memories active (128 Tb used) we start running out of virtual memory space.
//! That's why we count memories in use and try to drop them if we reach the limit.

use crossbeam::queue::{ArrayQueue, PopError};
use wasmtime::{Engine, Instance, Limits, Linker, Memory, MemoryType, Module, Store};
use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::imports::create_lunatic_imports;
use super::creator::MemoryChoice;
use super::ProcessEnvironment;
use crate::wasi::create_wasi_imports;

const MEMORIES_PER_STORE_LIMIT: usize = 20;
const TOTAL_MEMORIES_LIMIT: usize = 21 * 1024;

pub struct LinkerWithEnvironment {
    linker: Linker,
    environment: ProcessEnvironment,
    memories: usize,
}

impl LinkerWithEnvironment {
    pub fn instantiate(&mut self, module: &Module) -> Instance {
        self.linker.instantiate(module).unwrap()
    }
}

/// Wasmtime's store holds references to all instances, linkers, memories and tables created in it.
/// This resources will only be freed once the Store is gone.
pub struct LinkerPool {
    pool: ArrayQueue<LinkerWithEnvironment>,
    total_memories: AtomicUsize
}

unsafe impl Sync for LinkerPool {}

impl LinkerPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            pool: ArrayQueue::new(capacity),
            total_memories: AtomicUsize::new(0)
        }
    }

    pub fn get(
        &self,
        engine: Engine,
        module: Module,
        memory: MemoryChoice,
        yielder: usize,
    ) -> Result<LinkerWithEnvironment> {
        if self.total_memories.load(Ordering::SeqCst) >= TOTAL_MEMORIES_LIMIT {
            return Err(anyhow::anyhow!("Process limit reached: {}", TOTAL_MEMORIES_LIMIT));
        }

        // If we are not reusing a memory we will be allocating a new one.
        match memory {
            MemoryChoice::Existing(_) => (),
            MemoryChoice::New(_) => { self.total_memories.fetch_add(1, Ordering::SeqCst); }
        }

        let store_linker_env = match self.pool.pop() {
            Err(PopError) => {
                let store = Store::new(&engine);
                let mut linker = Linker::new(&store);
                linker.allow_shadowing(true);

                let mut memories = 0;
                let memory = match memory {
                    MemoryChoice::Existing(memory) => memory,
                    MemoryChoice::New(min_memory) => {
                        memories += 1;
                        let memory_ty = MemoryType::new(Limits::new(min_memory, None));
                        Memory::new(&store, memory_ty)
                    }
                };
                linker.define("lunatic", "memory", memory.clone())?;

                let environment = ProcessEnvironment::new(
                    engine,
                    module,
                    // memory,
                    yielder,
                );

                create_lunatic_imports(&mut linker, &environment)?;
                create_wasi_imports(&mut linker, &environment);

                LinkerWithEnvironment {
                    linker,
                    environment,
                    memories
                }
            }

            Ok(mut store_linker_env) => {
                let memory = match memory {
                    MemoryChoice::Existing(memory) => memory,
                    MemoryChoice::New(min_memory) => {
                        store_linker_env.memories += 1;
                        let memory_ty = MemoryType::new(Limits::new(min_memory, None));
                        Memory::new(store_linker_env.linker.store(), memory_ty)
                    }
                };
                store_linker_env.linker.define("lunatic", "memory", memory.clone())?;
                // store_linker_env.environment.set_memory(memory);
                store_linker_env.environment.set_yielder(yielder);
                store_linker_env
            }
        };
        
        Ok(store_linker_env)
    }

    pub fn recycle(&self, store_linker_env: LinkerWithEnvironment) {
        let memories = store_linker_env.memories;

        if memories < MEMORIES_PER_STORE_LIMIT {
            // If we push over the pool capacity just drop the StoreLinkerWithEnvironment.
            match self.pool.push(store_linker_env) {
                Ok(_) => (),
                Err(_) => {
                    self.total_memories.fetch_sub(memories, Ordering::SeqCst);
                }
            };
        } else {
            // Don't recycle stores with more than 20 memories in it.
            self.total_memories.fetch_sub(memories, Ordering::SeqCst);
        }
    }
}
