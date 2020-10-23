use super::{FunctionLookup, MemoryChoice, Process, ProcessEnvironment};

use crate::{Resource, ResourceTypeOwned, RESOURCES};

use anyhow::Result;
use smol::future::yield_now;
use wasmtime::Linker;

/// This is somewhat of a Lunatic stdlib definition. It creates HOST functions exposing
/// functionality provided by the runtime (filesystem, networking, process creation, etc).
/// It should be a complement to the WASI functions.
///
/// The HOST functions are implemented with closures that capturing the environment belonging
/// to the instance, like yielder address and memory pointers.
pub fn add_to_linker(linker: &mut Linker, environment: ProcessEnvironment) -> Result<()> {
    // Increments reference count on a resource
    linker.func("lunatic", "clone", move |index: u32| {
        RESOURCES.clone(index as usize);
    })?;

    // Decrements reference count on a resource
    linker.func("lunatic", "drop", move |index: u32| {
        RESOURCES.drop(index as usize);
    })?;

    // Yield this process allowing other to be scheduled on same thread.
    let env = environment.clone();
    linker.func("lunatic", "yield", move || env.async_(yield_now()))?;

    // Spawn new process and call a fuction from the function table under the `index` and pass one i32 argument.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "spawn",
        move |index: u32, argument: i64| -> u32 {
            let task = Process::spawn(
                env.engine(),
                env.module(),
                FunctionLookup::TableIndex((index, argument)),
                MemoryChoice::New(18),
            );
            let process = Process::from(task);
            RESOURCES.add(Resource::Owned(ResourceTypeOwned::Process(process))) as u32
        },
    )?;

    // Wait on chaild process to finish.
    let env = environment.clone();
    linker.func("lunatic", "join", move |index: u32| {
        match RESOURCES.take(index as usize) {
            Resource::Owned(ResourceTypeOwned::Process(process)) => {
                let task = process.take_task();
                let _ignore = env.async_(task);
            }
            _ => panic!("Only processes can be joined"),
        }
    })?;

    Ok(())
}
