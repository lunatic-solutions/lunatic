use super::{FunctionLookup, MemoryChoice, Process, ProcessEnvironment};

use anyhow::Result;
use smol::{future::yield_now, Timer};
use wasmtime::{ExternRef, Linker};

use std::time::{Duration, Instant};

/// This is somewhat of a Lunatic stdlib definition. It creates HOST functions exposing
/// functionality provided by the runtime (filesystem, networking, process creation, etc).
/// It should be a complement to the WASI functions.
///
/// The HOST functions are implemented with closures that capturing the environment belonging
/// to the instance, like yielder address and memory pointers.
pub fn add_to_linker(linker: &mut Linker, environment: &ProcessEnvironment) -> Result<()> {
    // Return a free slot in the externref table.
    let env = environment.clone();
    linker.func("lunatic", "get_externref_free_slot", move || -> i32 {
        env.get_externref_free_slot()
    })?;

    // Mark a slot as free in the externref table.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "set_externref_free_slot",
        move |free_slot: i32| {
            env.set_externref_free_slot(free_slot);
        },
    )?;

    // Clone externref slot
    linker.func(
        "lunatic",
        "clone_externref",
        move |externref: Option<ExternRef>| -> Option<ExternRef> { externref.clone() },
    )?;

    // Yield this process allowing other to be scheduled on same thread.
    let env = environment.clone();
    linker.func("lunatic", "yield", move || env.async_(yield_now()))?;

    // Suspend process for some time
    let env = environment.clone();
    linker.func("lunatic", "sleep_ms", move |millis: u64| {
        let now = Instant::now();
        let when = now + Duration::from_millis(millis);
        env.async_(Timer::at(when));
    })?;

    // Spawn new process and call a fuction from the function table under the `index` and pass one i32 argument.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "spawn",
        move |index: i32, argument1: i32, argument2: i64| -> Option<ExternRef> {
            let process = Process::spawn(
                env.engine(),
                env.module(),
                FunctionLookup::TableIndex((index, argument1, argument2)),
                MemoryChoice::New(18),
            );
            Some(ExternRef::new(process))
        },
    )?;

    // Wait on chaild process to finish.
    let env = environment.clone();
    linker.func("lunatic", "join", move |mut process: Option<ExternRef>| {
        let process = process.take().unwrap();
        let process = process.data();
        if let Some(process) = process.downcast_ref::<Process>() {
            let task = process.take_task().unwrap();
            let _ignore = env.async_(task);
        } else {
            panic!("Only processes can be joined");
        }
    })?;

    Ok(())
}
