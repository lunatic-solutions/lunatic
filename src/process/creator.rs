use lazy_static::lazy_static;
use smol::{Task, Executor};
use wasmtime::{Engine, Memory, Module, Val};
use anyhow::Result;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use super::pool::LinkerPool;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    static ref LINKER_ENV_POOL: LinkerPool = LinkerPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
}

/// Used to look up a functions by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument)
    TableIndex((i32, i32)),
    Name(&'static str),
}

#[derive(Clone)]
pub enum MemoryChoice {
    Existing(Memory),
    New(u32)
}

/// Spawn a new process
pub fn spawn(
    engine: Engine,
    module: Module,
    function: FunctionLookup,
    memory: MemoryChoice,
) -> Task<Result<()>> {
    let task = ASYNC_POOL
        .with_tls(&wasmtime_runtime::traphandlers::tls::PTR, move |yielder| {
            let yielder_ptr = &yielder as *const AsyncYielder<_> as usize;

            let mut store_linker_env
                = LINKER_ENV_POOL.get(engine.clone(), module.clone(), memory.clone(), yielder_ptr)?;

            let instance = store_linker_env.instantiate(&module);
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
            
            LINKER_ENV_POOL.recycle(store_linker_env);
            Ok(())
        });

    EXECUTOR.spawn(async {
        let mut task = task?;
        let result = (&mut task).await.unwrap();
        ASYNC_POOL.recycle(task);
        result
    })
}
