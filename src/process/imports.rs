use super::creator::{spawn, FunctionLookup, MemoryChoice};
use super::{ ProcessEnvironment, Process };

use smol::future::yield_now;
use wasmtime::Linker;
use anyhow::Result;

/// This is somewhat of a Lunatic stdlib definition. It creates HOST functions exposing
/// functionality provided by the runtime (filesystem, networking, process creation, etc).
/// It should be a complement to the WASI functions.
///
/// The HOST functions are implemented with closures that capturing the environment belonging
/// to the instance, like yielder address and memory pointers.
pub fn create_lunatic_imports(linker: &mut Linker, environment: ProcessEnvironment) -> Result<()> {
    // Yield this process allowing other to be scheduled on same thread.
    let env = environment.clone();
    linker.func("lunatic", "yield", move || env.async_(yield_now()))?;

    // Spawn new process and call a fuction from the function table under the `index` and pass one i32 argument.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "spawn",
        move |index: i32, argument: i32| -> i32 {
            let task = spawn(
                env.engine(),
                env.module(),
                FunctionLookup::TableIndex((index, argument)),
                MemoryChoice::New(18),
            );
            let process = Process::from(task);
            match env.processes.borrow_mut().insert(process) {
                None => -1,
                Some(id) => id as i32
            }
        },
    )?;

    // Wait on chaild process to finish.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "join",
        move |pid: i32| {
            if let Some(process) = env.processes.borrow_mut().get_mut(pid as usize) {
                env.async_(process.mut_task());
            }
        },
    )?;

    // Create a buffer and send it to the process with the `pid`
    linker.func(
        "lunatic",
        "send",
        |pid: i32, buffer: i32, len: i32| -> i32 { 0 },
    )?;

    // Receive buffer
    linker.func("lunatic", "receive", |buffer: i32, len: i32| -> i32 { 0 })?;

    Ok(())
}
