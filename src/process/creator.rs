use lazy_static::lazy_static;
use smol::{Task, Executor};
use wasmtime::{Engine, Module, Store, Linker, Memory, MemoryType, Limits, Val };
use anyhow::Result;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use super::ProcessEnvironment;
use super::imports::create_lunatic_imports;
use crate::wasi::create_wasi_imports;


lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
}

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument)
    TableIndex((i32, i32)),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances.
#[derive(Clone)]
pub enum MemoryChoice {
    Existing(Memory),
    New(u32)
}

/// Spawn a new process.
pub fn spawn(
    engine: Engine,
    module: Module,
    function: FunctionLookup,
    memory: MemoryChoice,
) -> Task<Result<()>> {
    let task = ASYNC_POOL
        .with_tls(&wasmtime_runtime::traphandlers::tls::PTR, move |yielder| {
            let yielder_ptr = &yielder as *const AsyncYielder<_> as usize;

            let store = Store::new(&engine);
            let mut linker = Linker::new(&store);

            let memory = match memory {
                MemoryChoice::Existing(memory) => memory,
                MemoryChoice::New(min_memory) => {
                    let memory_ty = MemoryType::new(Limits::new(min_memory, None));
                    Memory::new(&store, memory_ty)
                }
            };
            
            let environment = ProcessEnvironment::new(
                engine,
                module.clone(),
                memory.data_ptr(),
                yielder_ptr,
            );

            linker.define("lunatic", "memory", memory)?;
            create_lunatic_imports(&mut linker, environment.clone())?;
            create_wasi_imports(&mut linker, &environment);

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
                    func.call(&[Val::from(index), Val::from(argument)])?;                 
                }
            }
            
            Ok(())
        });

    EXECUTOR.spawn(async {
        let mut task = task?;
        let result = (&mut task).await.unwrap();
        ASYNC_POOL.recycle(task);
        result
    })
}
