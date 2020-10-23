pub mod api;

use anyhow::Result;
use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;
use lazy_static::lazy_static;
use smol::{Executor, Task};
use wasmtime::{Engine, Limits, Linker, Memory, MemoryType, Module, Store, Val};

use crate::channel;
use crate::wasi;

use std::future::Future;
use std::mem::ManuallyDrop;

lazy_static! {
    static ref WORMHOLE_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
}

pub type AsyncYielderCast<'a> = AsyncYielder<'a, Result<()>>;

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument)
    TableIndex((u32, i64)),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances (once the WASM multi-threading proposal is supported in Wasmtime).
#[derive(Clone)]
pub enum MemoryChoice {
    Existing(Memory),
    New(u32),
}

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a raw pointer to Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe to have a
/// raw pointer to its memory content and only use it inside of host functions.
#[derive(Clone)]
pub struct ProcessEnvironment {
    engine: Engine,
    module: Module,
    memory: *mut u8,
    yielder: usize,
}

impl ProcessEnvironment {
    pub fn new(engine: Engine, module: Module, memory: *mut u8, yielder: usize) -> Self {
        Self {
            engine,
            module,
            memory,
            yielder,
        }
    }

    /// Run an async future and return the output when done.
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder =
            unsafe { std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielderCast>) };
        yielder.async_suspend(future)
    }

    pub fn memory(&self) -> *mut u8 {
        self.memory
    }

    pub fn engine(&self) -> Engine {
        self.engine.clone()
    }

    pub fn module(&self) -> Module {
        self.module.clone()
    }
}

/// A lunatic process represents an actor.
pub struct Process {
    task: Task<Result<()>>,
}

impl Process {
    pub fn take_task(self) -> Task<Result<()>> {
        self.task
    }

    /// Spawn a new process.
    pub fn spawn(
        engine: Engine,
        module: Module,
        function: FunctionLookup,
        memory: MemoryChoice,
    ) -> Self {
        let process = WORMHOLE_POOL.with_tls(
            [&wasmtime_runtime::traphandlers::tls::PTR],
            move |yielder| {
                let yielder_ptr = &yielder as *const AsyncYielderCast as usize;

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
                self::api::add_to_linker(&mut linker, environment.clone())?;
                channel::api::add_to_linker(&mut linker, environment.clone())?;
                wasi::api::add_to_linker(&mut linker, &environment)?;

                let instance = linker.instantiate(&module)?;
                match function {
                    FunctionLookup::Name(name) => {
                        let func = instance.get_func(name).unwrap();
                        let now = std::time::Instant::now();
                        func.call(&[])?;
                        println!("Elapsed time: {} ms", now.elapsed().as_millis());
                    }
                    FunctionLookup::TableIndex((index, argument)) => {
                        let func = instance.get_func("lunatic_spawn_by_index").unwrap();
                        func.call(&[Val::from(index as i32), Val::from(argument)])?;
                    }
                }

                Ok(())
            },
        );

        let task = EXECUTOR.spawn(async {
            let mut process = process?;
            let result = (&mut process).await.unwrap();
            WORMHOLE_POOL.recycle(process);
            result
        });

        Self { task }
    }
}
