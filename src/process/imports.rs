use super::channel::Channel;
use super::creator::{spawn, FunctionLookup, MemoryChoice};
use super::{
    Process, ProcessEnvironment, Resource, ResourceRc, ResourceTypeClonable, ResourceTypeOwned,
    RESOURCES,
};

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
            let task = spawn(
                env.engine(),
                env.module(),
                FunctionLookup::TableIndex((index, argument)),
                MemoryChoice::New(18),
            );
            let process = Process::from(task);
            RESOURCES.create(Resource::Owned(ResourceTypeOwned::Process(process))) as u32
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

    // Create a channel
    linker.func("lunatic", "channel", |bound: u32| -> u32 {
        let channel = Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        });
        let resource_rc = ResourceRc {
            resource: ResourceTypeClonable::Channel(channel),
            count: 1,
        };
        RESOURCES.create(Resource::Clonable(resource_rc)) as u32
    })?;

    // Create a buffer and send it to a channel
    let env = environment.clone();
    linker.func("lunatic", "send", move |index: u32, iovec: u32| {
        RESOURCES.with_resource(index as usize, |resource| match resource {
            Resource::Clonable(resource_rc) => match &resource_rc.resource {
                ResourceTypeClonable::Channel(channel) => {
                    let iovec = WasiIoVec::from(env.memory(), iovec as usize);
                    let future = channel.send(iovec.as_slice());
                    env.async_(future);
                }
            },
            _ => panic!("Only channels can be sent to"),
        });
    })?;

    // Receive buffer and write it to memory
    let env = environment.clone();
    linker.func("lunatic", "receive", move |index: u32, iovec: u32| {
        RESOURCES.with_resource(index as usize, |resource| match resource {
            Resource::Clonable(resource_rc) => match &resource_rc.resource {
                ResourceTypeClonable::Channel(channel) => {
                    let mut iovec = WasiIoVec::from(env.memory(), iovec as usize);
                    let future = channel.recieve();
                    let buffer = env.async_(future).unwrap();
                    // TODO: Check for length of buffer before writing to it.
                    buffer.give_to(iovec.as_mut_slice().as_mut_ptr());
                }
            },
            _ => panic!("Only channels can be sent to"),
        });
    })?;

    Ok(())
}
