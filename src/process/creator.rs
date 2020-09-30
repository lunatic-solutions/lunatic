use std::cell::RefCell;
use std::rc::Rc;

use wasmer::{Store, Module, Instance, Function, Memory, MemoryType, ImportObject, Exports};
use tokio::task::{JoinHandle, yield_now};
use lazy_static::lazy_static;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use crate::process::Process;
use crate::wasi::create_wasi_imports;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
}

#[derive(Clone)]
pub struct ImportEnv {
    pub process: Rc<RefCell<Option<Process>>>
}

/// Spawns a new process by creating a WASM instance from the `module` with `min_memory` of WASM memory pages
/// and call the exported `function` inside a new tokio task.
pub fn spawn_by_name(module: Module, function: &'static str, min_memory: u32) -> JoinHandle<()> {
    let mut task = ASYNC_POOL.with_tls(
        &wasmtime_runtime::traphandlers::tls::PTR, // TODO: Update to
        move |yielder|
    {
        let memory_ty = MemoryType::new(min_memory, None, false);
        let memory = Memory::new(module.store(), memory_ty).unwrap();

        let mut resolver = ImportObject::new();
        let import_env = ImportEnv { process: Rc::new(RefCell::new(None)) };
        create_lunatic_imports(module.store(), &mut resolver, import_env.clone(), memory.clone());
        create_wasi_imports(module.store().clone(), &mut resolver, import_env.clone());

        let instance = Instance::new(&module, &resolver).unwrap();
        let func = instance.exports
            .get_function(function).unwrap()
            .native::<(),()>().unwrap();

        let process = Process {
            instance,
            memory,
            yielder: &yielder as *const AsyncYielder<()> as usize
        };
        { *import_env.process.borrow_mut() = Some(process); }

        let now = std::time::Instant::now();
        func.call().unwrap();
        println!("Elapsed time: {} ms", now.elapsed().as_millis());
    }).unwrap();

    tokio::spawn(async move {
        (&mut task).await;
        ASYNC_POOL.recycle(task);
    })
}

/// Spawn new process from an existing one and use the function with the `index` in the main table as
/// entrance point (passing `argument` as the 1st argument to the function). If `share_memory` is true
/// the new process will share memory with the old one.
pub fn spawn_by_index(process: Process, index: i32, argument: i32, share_memory: bool) -> JoinHandle<()> {
    let mut task = ASYNC_POOL.with_tls(
        &wasmtime_runtime::traphandlers::tls::PTR, // TODO: Update to
        move |yielder|
    {
        let memory = if !share_memory {
            let min_memory = process.memory.ty().minimum;
            let memory_ty = MemoryType::new(min_memory, None, false);
            Memory::new(process.instance.store(), memory_ty).unwrap()
        } else {
            process.memory
        };

        let mut resolver = ImportObject::new();
        let import_env = ImportEnv { process: Rc::new(RefCell::new(None)) };
        create_lunatic_imports(process.instance.store(), &mut resolver, import_env.clone(), memory.clone());
        create_wasi_imports(process.instance.store().clone(), &mut resolver, import_env.clone());

        let instance = Instance::new(&process.instance.module(), &resolver).unwrap();
        let func = instance.exports
            .get_function("lunatic_spawn_by_index").unwrap()
            .native::<(i32, i32),()>().unwrap();

        let process = Process {
            instance,
            memory,
            yielder: &yielder as *const AsyncYielder<()> as usize
        };
        { *import_env.process.borrow_mut() = Some(process); }

        func.call(index, argument).unwrap();
    }).unwrap();

    tokio::spawn(async move {
        (&mut task).await;
        ASYNC_POOL.recycle(task);
    })
}

/// Add all imports provided by the Lunatic runtime to this instance.
fn create_lunatic_imports(store: &Store, resolver: &mut ImportObject, import_env: ImportEnv, memory: Memory) {
    // Lunatic stdlib
    let mut lunatic_env = Exports::new();
    lunatic_env.insert("memory", memory);

    fn yield_(env: &mut ImportEnv) {
        env.process.borrow().as_ref().unwrap().async_( yield_now() );
    }
    lunatic_env.insert(
        "yield",
        Function::new_native_with_env(store, import_env.clone(), yield_)
    );

    fn spawn(env: &mut ImportEnv, index: i32, argument: i32) {
        spawn_by_index(
            env.process.borrow().as_ref().unwrap().clone(),
            index,
            argument,
            false
        );
    }
    lunatic_env.insert(
        "spawn",
        Function::new_native_with_env(store, import_env.clone(), spawn)
    );

    resolver.register("lunatic", lunatic_env);
}