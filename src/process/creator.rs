use lazy_static::lazy_static;
use tokio::task::JoinHandle;
use wasmtime::{Engine, Memory, Module, Val};

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use super::pool::StoreLinkerPool;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    static ref STORE_LINKER_ENV_POOL: StoreLinkerPool = StoreLinkerPool::new(128);
}

/// Used to look up a functions by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument)
    TableIndex((i32, i32)),
    Name(&'static str),
}

/// Spawn a new task
pub fn spawn(
    engine: Engine,
    module: Module,
    function: FunctionLookup,
    memory: Option<Memory>,
) -> JoinHandle<()> {
    let mut task = ASYNC_POOL
        .with_tls(&wasmtime_runtime::traphandlers::tls::PTR, move |yielder| {
            let yielder = &yielder as *const AsyncYielder<_> as usize;
            let store_linker_env =
                STORE_LINKER_ENV_POOL.get(engine, module.clone(), memory, yielder);

            let instance = store_linker_env.instantiate(&module);
            match function {
                FunctionLookup::Name(name) => {
                    let func = instance.get_func(name).unwrap();
                    let now = std::time::Instant::now();
                    func.call(&[]).unwrap();
                    println!("Elapsed time: {} ms", now.elapsed().as_millis());
                }
                FunctionLookup::TableIndex((index, argument)) => {
                    let func = instance.get_func("lunatic_spawn_by_index").unwrap();
                    func.call(&[Val::from(index), Val::from(argument)]).unwrap();
                }
            }
            STORE_LINKER_ENV_POOL.recycle(store_linker_env);
        })
        .unwrap();

    let join_handle = tokio::spawn(async move {
        (&mut task).await;
        ASYNC_POOL.recycle(task);
    });
    join_handle
}
