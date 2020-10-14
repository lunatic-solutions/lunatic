use super::channel::Channel;
use super::creator::{spawn, FunctionLookup, MemoryChoice};
use super::{Process, ProcessEnvironment, Resource, RESOURCES};

use crate::wasi::types::*;

use anyhow::Result;
use smol::future::yield_now;
use wasmtime::Linker;

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

            RESOURCES.create(Resource::Process(process)) as i32
        },
    )?;

    // Wait on chaild process to finish.
    let env = environment.clone();
    linker.func("lunatic", "join", move |index: i32| {
        match RESOURCES.get(index as usize) {
            Resource::Process(mut process) => {
                let _ignore = env.async_(process.mut_task());
            }
            _ => panic!("Only processes can be joined"),
        }
    })?;

    // Create a channel
    linker.func("lunatic", "channel", |bound: i32| -> i32 {
        let channel = Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        });
        RESOURCES.create(Resource::Channel(channel)) as i32
    })?;

    // Create a buffer and send it to a channel
    let env = environment.clone();
    linker.func("lunatic", "send", move |index: i32, iovec: i32| {
        let iovec = WasiIoVec::from(env.memory(), iovec as usize);
        match RESOURCES.get(index as usize) {
            Resource::Channel(channel) => {
                let future = channel.send(iovec.as_slice());
                env.async_(future);
            }
            _ => panic!("Only channels can be sent to"),
        }
    })?;

    // Receive buffer and write it to memory
    let env = environment.clone();
    linker.func("lunatic", "receive", move |index: i32, iovec: i32| {
        let mut iovec = WasiIoVec::from(env.memory(), iovec as usize);
        match RESOURCES.get(index as usize) {
            Resource::Channel(channel) => {
                let future = channel.recieve();
                let buffer = env.async_(future).unwrap();
                // TODO: Check for length of buffer before writing to it.
                buffer.give_to(iovec.as_mut_slice().as_mut_ptr());
            }
            _ => panic!("Only channels can be received to"),
        }
    })?;

    Ok(())
}
