use super::{ ProcessEnvironment, Process, CHANNELS };
use super::creator::{spawn, FunctionLookup, MemoryChoice};
use super::channel::Channel;

use crate::wasi::types::*;

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
                let _ignore = env.async_(process.mut_task());
            }
        },
    )?;

    // Create a channel
    linker.func(
        "lunatic",
        "channel",
        |bound: i32| -> i32 {
            let channel = Channel::new(if bound > 0 {Some (bound as usize)} else {None});
            match CHANNELS.insert(channel) {
                None => -1,
                Some(id) => id as i32
            }
        },
    )?;

    // Create a buffer and send it to a channel
    let env = environment.clone();
    linker.func(
        "lunatic",
        "send",
        move |channel_id: i32, iovec: i32| {
            let iovec = WasiIoVec::from(env.memory(), iovec);
            let channel = CHANNELS.get(channel_id as usize).unwrap();
            let future = channel.send(iovec.as_slice());
            env.async_(future);
        },
    )?;

    // Receive buffer and write it to memory
    let env = environment.clone();
    linker.func("lunatic", "receive",
        move |channel_id: i32, iovec: i32| {
            let mut iovec = WasiIoVec::from(env.memory(), iovec);
            let channel = CHANNELS.get(channel_id as usize).unwrap();
            let future = channel.recieve();
            let buffer = env.async_(future).unwrap();
            // TODO: Check for length of buffer before writing to it.
            buffer.give_to(iovec.as_mut_slice().as_mut_ptr());
        }
    )?;

    Ok(())
}
