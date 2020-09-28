use wasmtime::{Engine, Linker, Module, Store, Memory, MemoryType, Limits};
use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use std::mem::ManuallyDrop;

use lazy_static::lazy_static;
use tokio::task::yield_now;
use crate::library::LunaticLib;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
}

#[derive(Clone)]
pub struct Spawner {
    module: Module,
    store: Store,
    engine: Engine,
    memory: Memory
}

impl Spawner {
    pub fn new(module: Module, engine: Engine, initial_memory_size: u32) -> Self {
        let store = Store::new(&engine);
        let memory_ty = MemoryType::new(Limits::new(initial_memory_size, None));
        let memory = Memory::new(&store, memory_ty);
        Self { module, store, engine, memory }
    }

    pub fn spawn_by_name(&self, function: &'static str) -> tokio::task::JoinHandle<()> {
        let spawner = self.clone();
        let mut task = ASYNC_POOL.with_tls(
            &wasmtime_runtime::traphandlers::tls::PTR,
            move |yielder|
        {
            let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;

            let mut linker = Linker::new(&spawner.store);
            spawner.add_to_linker(yielder_ptr, &mut linker);
            linker.define("lunatic", "memory", spawner.memory).unwrap();

            let instance = linker.instantiate(&spawner.module).unwrap();
            let func = instance
                .get_func(function)
                .ok_or(anyhow::format_err!("failed to find `{}` function export", function)).unwrap()
                .get0::<()>().unwrap();

            let now = std::time::Instant::now();
            func().unwrap();
            println!("Elapsed time: {} ms", now.elapsed().as_millis());
        }).unwrap();

        tokio::spawn(async move {
            (&mut task).await;
            ASYNC_POOL.recycle(task);
        })
    }

    // Settin share_memory to true is not thread safe currently in Wasmtime. Stores can't be moved between threads,
    // but instances must be moved, so we create a new store per instance. And memory must be coupled with stores.
    // This forces us to create a new memory per instance. TODO: Check Wasmtime support for threading.
    pub fn spawn_by_index(&self, index: i32, share_memory: bool) {
        // let mut spawner = self.clone();

        // if !share_memory {
        //     spawner.store = Store::new(&spawner.engine);
        //     let initial_memory_size = spawner.memory.ty().limits().min();
        //     let memory_ty = MemoryType::new(Limits::new(initial_memory_size, None));
        //     let memory = Memory::new(&spawner.store, memory_ty);
        //     spawner.memory = memory;
        // }

        // let mut linker = Linker::new(&spawner.store);
        // linker.define("lunatic", "memory", spawner.memory.clone()).unwrap();

        let mut task = ASYNC_POOL.with_tls(
            &wasmtime_runtime::traphandlers::tls::PTR,
            move |yielder|
        {
            let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;
            // spawner.add_to_linker(yielder_ptr, &mut linker);

            // let instance = linker.instantiate(&spawner.module).unwrap();
            // let func = instance
            //     .get_func("lunatic_spawn_by_index")
            //     .ok_or(anyhow::format_err!("failed to find `hello`  function export")).unwrap()
            //     .get1::<i32, ()>().unwrap();
        //     func(index).unwrap();
        }).unwrap();
        tokio::spawn(async move {
            (&mut task).await;
            ASYNC_POOL.recycle(task);
        });
    }
}

impl LunaticLib for Spawner {
    // TODO: Rename to externref_functions
    fn functions(&self) -> Vec<&'static str> {
        vec!["yield", "spawn", "spawn_module", ]
    }

    fn add_to_linker(&self, yielder_ptr: usize, linker: &mut Linker) {
        // yield() suspends this async execution until the scheduler picks it up again.
        linker.func("lunatic", "yield", move || {
            let mut yielder = unsafe {
                std::ptr::read(yielder_ptr as *const ManuallyDrop<AsyncYielder<()>>)
            };

            yielder.async_suspend( yield_now() );
        }).unwrap();

        // spawn(index: i32) spawns a new instance and calls a function with with the index.
        let spawner = self.clone();
        linker.func("lunatic", "spawn", move |index: i32| {
            spawner.spawn_by_index(index, false);
        }).unwrap();


        // WASI temp definitions
        linker.func("wasi_snapshot_preview1", "proc_exit", move |exit_code: i32| {
            println!("wasi_snapshot_preview1:proc_exit({}) called!", exit_code);
            std::process::exit(exit_code);
        }).unwrap();
        linker.func("wasi_snapshot_preview1", "fd_write", move |_: i32, _: i32, _: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:fd_write umimplemented!"); 0}).unwrap();
        linker.func("wasi_snapshot_preview1", "fd_prestat_get", move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:fd_prestat_get umimplemented!");
            8 // WASI_EBADF
        }).unwrap();
        linker.func("wasi_snapshot_preview1", "fd_prestat_dir_name", move |_: i32, _: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:fd_prestat_dir_name umimplemented!");
            28 // WASI_EINVAL
        }).unwrap();
        linker.func("wasi_snapshot_preview1", "environ_sizes_get", move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:environ_sizes_get umimplemented!"); 0}).unwrap();
        linker.func("wasi_snapshot_preview1", "environ_get", move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:environ_get umimplemented!"); 0}).unwrap();
    }
}
