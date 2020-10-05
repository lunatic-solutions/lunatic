use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use wasmer::{Store, Module, Instance, Function, Memory, MemoryType, ImportObject, Exports};
use tokio::task::{JoinHandle, yield_now};
use tokio::sync::mpsc::channel;
use lazy_static::lazy_static;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use crate::process::{Process, ProcessInformation, ProcessStatus, AllProcesses};
use crate::wasi::create_wasi_imports;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    static ref PROCESSES: AllProcesses = AllProcesses::new();
}

#[derive(Clone)]
pub struct ImportEnv {
    pub process: Rc<RefCell<Option<Process>>>
}

/// Reserves a process information slot it the global `PROCESSES` array and keeps track of free slots.
/// TODO: This way of ensuring there is a limited amount of active processes is flawed, because 2 processes can have the same ID.
/// If we join() by id at some point after the process has give up this slot, we would be joining on the wrong process id.
/// We need to have a truely unique combination of (slot & id), where id is globaly unique. In this case if we don't find this id
/// in the slot we know that this process is DONE.
fn create_process_information() -> usize {
        let maybe_empty = { PROCESSES.free_slots.write().unwrap().pop() };
        if let Some(empty_id) = maybe_empty {
            let mut processes = PROCESSES.processes.write().unwrap();
            let current_size = processes.len();
            let pi = ProcessInformation { status: ProcessStatus::INIT, sender: None, join_handle: None};
            if empty_id + 1 > current_size {
                processes.push(Mutex::new(pi));
            } else {
                processes[empty_id] = Mutex::new(pi);
            }
            empty_id
        } else {
            // If there are no empty slots left, try to find unused finished processes and declare their slots as empty.
            {
                let processes = PROCESSES.processes.read().unwrap();
                let mut free_slots = PROCESSES.free_slots.write().unwrap();

                processes.iter().enumerate().for_each(|(id, process)| {
                    let lock = process.try_lock();
                    if let Ok(process) = lock {
                        if process.is_done() {
                            free_slots.push(id)
                        }
                    }
                });
            }
            // Attempt again to get an empty slot after the GC cycle
            create_process_information()
        }
}

/// Spawns a new process by creating a WASM instance from the `module` with `min_memory` of WASM memory pages
/// and call the exported `function` inside a new tokio task.
pub fn spawn_by_name(module: Module, function: &'static str, min_memory: u32) -> JoinHandle<()> {
    let id = create_process_information();

    let mut task = ASYNC_POOL.with_tls(
        &wasmtime_runtime::traphandlers::tls::PTR, // TODO: Update to
        move |yielder|
    {
        let memory_ty = MemoryType::new(min_memory, None, false);
        let memory = Memory::new(module.store(), memory_ty)?;

        let mut resolver = ImportObject::new();
        let import_env = ImportEnv { process: Rc::new(RefCell::new(None)) };
        create_lunatic_imports(module.store(), &mut resolver, import_env.clone(), memory.clone());
        create_wasi_imports(module.store().clone(), &mut resolver, import_env.clone());

        let yielder = &yielder as *const AsyncYielder<Result<_, _>> as usize;
        let instance = create_instance(id, &module, memory, yielder)?;
        let func = instance.exports
            .get_function(function).unwrap()
            .native::<(),()>().unwrap();

        let now = std::time::Instant::now();
        func.call()?;
        println!("Elapsed time: {} ms", now.elapsed().as_millis());

        Ok(())
    }).unwrap();

    let handle = tokio::spawn(async move {
        let result = (&mut task).await;
        ASYNC_POOL.recycle(task);
        let processes = PROCESSES.processes.read().unwrap();
        processes.get(id).unwrap().lock().unwrap().status = ProcessStatus::DONE(result.unwrap());
    });
    handle
}

/// Spawn new process from an existing one and use the function with the `index` in the main table as
/// entrance point (passing `argument` as the 1st argument to the function). If `share_memory` is true
/// the new process will share memory with the old one.
pub fn spawn_by_index(process: Process, index: i32, argument: i32, share_memory: bool) -> usize {
    let id = create_process_information();

    let mut task = ASYNC_POOL.with_tls(
        &wasmtime_runtime::traphandlers::tls::PTR, // TODO: Update to
        move |yielder|
    {
        // Make sure everything is dropped before marking the process as DONE
        {
            let memory = if !share_memory {
                let min_memory = process.memory.ty().minimum;
                let memory_ty = MemoryType::new(min_memory, None, false);
                Memory::new(process.module.store(), memory_ty).unwrap()
            } else {
                process.memory.clone()
            };

            let yielder = &yielder as *const AsyncYielder<Result<_, _>> as usize;
            let instance = create_instance(id, &process.module, memory, yielder)?;

            let func = instance.exports
                .get_function("lunatic_spawn_by_index").unwrap()
                .native::<(i32, i32), ()>().unwrap();
            func.call(index, argument)?;
            Ok(())
        }
    }).unwrap();

    let join_handle = tokio::spawn(async move {
        let result = (&mut task).await;
        ASYNC_POOL.recycle(task);
        let processes = PROCESSES.processes.read().unwrap();
        processes.get(id).unwrap().lock().unwrap().status = ProcessStatus::DONE(result.unwrap());
    });
    PROCESSES.processes.read().unwrap().get(id).unwrap().lock().unwrap().join_handle = Some(join_handle);
    id
}

/// Creates a new wasm instance and associates it with a process id and memory
fn create_instance(id: usize, module: &Module, memory: Memory, yielder: usize) -> Result<Instance, wasmer::InstantiationError> {
    let mut resolver = ImportObject::new();
    let import_env = ImportEnv { process: Rc::new(RefCell::new(None)) };
    create_lunatic_imports(module.store(), &mut resolver, import_env.clone(), memory.clone());
    create_wasi_imports(module.store().clone(), &mut resolver, import_env.clone());

    let instance = Instance::new(&module, &resolver)?;

    let (sender, receiver) = channel(100);
    let process = Process {
        id,
        module: module.clone(),
        memory,
        receiver: Arc::new(receiver),
        yielder
    };
    { *import_env.process.borrow_mut() = Some(process); 
    }

    {
        let processes = PROCESSES.processes.read().unwrap();
        processes.get(id).unwrap().lock().unwrap().sender = Some(sender);
        processes.get(id).unwrap().lock().unwrap().status = ProcessStatus::RUNNING;
    };

    Ok(instance)
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

    fn spawn(env: &mut ImportEnv, index: i32, argument: i32) -> i32 {
        spawn_by_index(
            env.process.borrow().as_ref().unwrap().clone(),
            index,
            argument,
            false
        ) as i32
    }
    lunatic_env.insert(
        "spawn",
        Function::new_native_with_env(store, import_env, spawn)
    );

    resolver.register("lunatic", lunatic_env);
}