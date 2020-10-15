use anyhow::Result;
use smol::Task;
use wasmtime::{Engine, Limits, Linker, Memory, MemoryType, Module, Store, Val};

use super::imports::create_lunatic_imports;
use super::{AsyncYielderCast, ProcessEnvironment, ASYNC_POOL, EXECUTOR};
use crate::wasi::create_wasi_imports;

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument)
    TableIndex((i32, i64)),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances.
#[derive(Clone)]
pub enum MemoryChoice {
    Existing(Memory),
    New(u32),
}

/// Spawn a new process.
pub fn spawn(
    engine: Engine,
    module: Module,
    function: FunctionLookup,
    memory: MemoryChoice,
) -> Task<Result<()>> {
    let task = ASYNC_POOL.with_tls(
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
            create_lunatic_imports(&mut linker, environment.clone())?;
            create_wasi_imports(&mut linker, &environment)?;

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
        },
    );

    EXECUTOR.spawn(async {
        let mut task = task?;
        let result = (&mut task).await.unwrap();
        ASYNC_POOL.recycle(task);
        result
    })
}
