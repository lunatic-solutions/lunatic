use wasmer::{Module, Store, Instance, Memory, Function, FunctionType, Type, MemoryType, ImportObject,
             Value, Exports};
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
    memory: Memory,
    imports: ImportObject,
}

impl Spawner {
    pub fn new(module: Module, store: Store, initial_memory_size: u32) -> Self {
        let memory_ty = MemoryType::new(initial_memory_size, None, false);
        let memory = Memory::new(&store, memory_ty).unwrap();
        let imports = ImportObject::new();

        Self { module, store, memory, imports }
    }

    pub fn spawn_by_name(&self, function: &'static str) -> tokio::task::JoinHandle<()> {
        let mut spawner = self.clone();
        let mut task = ASYNC_POOL.with_tls(
            &wasmtime_runtime::traphandlers::tls::PTR,
            move |yielder|
        {
            let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;

            let mut wasi_env = Exports::new();
            spawner.add_wasi_fake(yielder_ptr, &mut wasi_env);
            spawner.imports.register("wasi_snapshot_preview1", wasi_env);

            let mut lunatic_env = Exports::new();
            spawner.add_to_imports(yielder_ptr, &mut lunatic_env);
            lunatic_env.insert("memory", spawner.memory);
            spawner.imports.register("lunatic", lunatic_env);

            let instance = Instance::new(&spawner.module, &spawner.imports).unwrap();
            let func = instance.exports.get_function(function).unwrap()
                .native::<(),()>().unwrap();

            let now = std::time::Instant::now();
            func.call().unwrap();
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
        let mut spawner = self.clone();

        if !share_memory {
            let initial_memory_size = spawner.memory.ty().minimum;
            let memory_ty = MemoryType::new(initial_memory_size, None, false);
            let memory = Memory::new(&spawner.store, memory_ty).unwrap();
            spawner.memory = memory;
        }

        let instance = Instance::new(&spawner.module, &spawner.imports).unwrap();
        let func = instance.exports.get_function("lunatic_spawn_by_index").unwrap()
                .native::<i32,()>().unwrap();

        let mut task = ASYNC_POOL.with_tls(
            &wasmtime_runtime::traphandlers::tls::PTR,
            move |yielder|
        {
            let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;
            func.call(index).unwrap();
        }).unwrap();
        tokio::spawn(async move {
            (&mut task).await;
            ASYNC_POOL.recycle(task);
        });
    }

    fn add_wasi_fake(&self, yielder_ptr: usize, imports: &mut Exports) {
        let signature = FunctionType::new(vec![Type::I32], vec![]);
        imports.insert("proc_exit", Function::new(&self.store, &signature,
            move |args| {
                println!("wasi_snapshot_preview1:proc_exit({}) called!", args[0].unwrap_i32());
                std::process::exit(args[0].unwrap_i32());
            }
        ));

        let signature = FunctionType::new(
            vec![Type::I32, Type::I32, Type::I32, Type::I32], vec![Type::I32]
        );
        imports.insert("fd_write", Function::new(&self.store, &signature,
            move |_args| {
                println!("wasi_snapshot_preview1:fd_write() called!");
                Ok(vec![Value::I32(0)])
            }
        ));

        let signature = FunctionType::new(
            vec![Type::I32, Type::I32], vec![Type::I32]
        );
        imports.insert("fd_prestat_get", Function::new(&self.store, &signature,
            move |_args| {
                println!("wasi_snapshot_preview1:fd_prestat_get() called!");
                Ok(vec![Value::I32(8)]) // WASI_EBADF
            }
        ));

        let signature = FunctionType::new(
            vec![Type::I32, Type::I32, Type::I32], vec![Type::I32]
        );
        imports.insert("fd_prestat_dir_name", Function::new(&self.store, &signature,
            move |_args| {
                println!("wasi_snapshot_preview1:fd_prestat_dir_name() called!");
                Ok(vec![Value::I32(28)]) // WASI_EINVAL
            }
        ));

        let signature = FunctionType::new(
            vec![Type::I32, Type::I32], vec![Type::I32]
        );
        imports.insert("environ_sizes_get", Function::new(&self.store, &signature,
            move |_args| {
                println!("wasi_snapshot_preview1:environ_sizes_get() called!");
                Ok(vec![Value::I32(0)])
            }
        ));

        let signature = FunctionType::new(
            vec![Type::I32, Type::I32], vec![Type::I32]
        );
        imports.insert("environ_get", Function::new(&self.store, &signature,
            move |_args| {
                println!("wasi_snapshot_preview1:environ_get() called!");
                Ok(vec![Value::I32(0)])
            }
        ));
    }
}

impl LunaticLib for Spawner {
    // TODO: Rename to externref_functions
    fn functions(&self) -> Vec<&'static str> {
        vec!["yield", "spawn", "spawn_module", ]
    }

    fn add_to_imports(&self, yielder_ptr: usize, imports: &mut Exports) {
        // yield() suspends this async execution until the scheduler picks it up again.
        let signature = FunctionType::new(vec![], vec![]);
        imports.insert("yield", Function::new(&self.store, &signature,
            move |_args| {
                let mut yielder = unsafe {
                    std::ptr::read(yielder_ptr as *const ManuallyDrop<AsyncYielder<()>>)
                };
    
                yielder.async_suspend( yield_now() );
                Ok(vec![])
            }
        ));


        // spawn(index: i32) spawns a new instance and calls a function with with the index.
        let spawner = self.clone();
        let signature = FunctionType::new(vec![Type::I32], vec![]);
        imports.insert("spawn", Function::new(&self.store, &signature,
             move |args| {
                spawner.spawn_by_index(args[0].unwrap_i32(), true);
                Ok(vec![])
            }
        ));
    }
}
