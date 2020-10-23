use super::Channel;
use crate::process::ProcessEnvironment;
use crate::wasi::types::*;
use crate::{Resource, ResourceRc, ResourceTypeCloneable, RESOURCES};

use anyhow::Result;
use wasmtime::Linker;

/// Expose channel API to WebAssembly guests through the `lunatic` namespace.
pub fn add_to_linker(linker: &mut Linker, environment: ProcessEnvironment) -> Result<()> {
    // Create a channel
    linker.func("lunatic", "channel", |bound: u32| -> u32 {
        let channel = Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        });
        let resource_rc = ResourceRc {
            resource: ResourceTypeCloneable::Channel(channel),
            count: 1,
        };
        RESOURCES.add(Resource::Cloneable(resource_rc)) as u32
    })?;

    // Create a buffer and send it to a channel
    let env = environment.clone();
    linker.func("lunatic", "send", move |index: u32, iovec: u32| {
        RESOURCES.with_resource(index as usize, |resource| match resource {
            Resource::Cloneable(resource_rc) => match &resource_rc.resource {
                ResourceTypeCloneable::Channel(channel) => {
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
            Resource::Cloneable(resource_rc) => match &resource_rc.resource {
                ResourceTypeCloneable::Channel(channel) => {
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
