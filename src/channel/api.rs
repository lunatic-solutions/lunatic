use super::Channel;
use crate::process::ProcessEnvironment;
use crate::wasi::types::*;

use anyhow::Result;
use wasmtime::{ExternRef, Func, Linker, Val};

/// Expose channel API to WebAssembly guests through the `lunatic` namespace.
/// TODO: Transform all the panics into traps (wrong argument)
pub fn add_to_linker(linker: &mut Linker, environment: &ProcessEnvironment) -> Result<()> {
    // Create a channel
    // TODO: Return sender, receiver as multivalue once this lands:
    // TODO: https://github.com/bytecodealliance/wasmtime/issues/1178
    linker.func("lunatic", "channel", |bound: u32| -> Option<ExternRef> {
        let channel = Channel::new(if bound > 0 {
            Some(bound as usize)
        } else {
            None
        });
        Some(ExternRef::new(channel))
    })?;

    // Serializes an Externref containing a channel as an id.
    // Memory leak: If the value in never deserialized, this will leak memory.
    linker.func(
        "lunatic",
        "channel_serialize",
        move |mut channel: Option<ExternRef>| -> i64 {
            let channel = channel.take().unwrap();
            let channel = channel.data();
            if let Some(channel) = channel.downcast_ref::<Channel>() {
                channel.clone().serialize() as i64
            } else {
                panic!("Argument is not a channel")
            }
        },
    )?;

    // Deserializes a pointer as an Externref.
    linker.func(
        "lunatic",
        "channel_deserialize",
        move |channel_ptr: i64| -> Option<ExternRef> {
            match Channel::deserialize(channel_ptr as usize) {
                Some(channel) => Some(ExternRef::new(channel)),
                None => None,
            }
        },
    )?;

    // Create a buffer and send it to a channel
    let env = environment.clone();
    linker.func(
        "lunatic",
        "channel_send",
        move |mut channel: Option<ExternRef>, iovec: u32| {
            let channel = channel.take().unwrap();
            let channel = channel.data();
            if let Some(channel) = channel.downcast_ref::<Channel>() {
                let iovec = WasiIoVec::from_ptr(env.memory(), iovec as usize);
                let future = channel.send(iovec.as_slice());
                env.async_(future);
            } else {
                panic!("Only channels can be sent to")
            }
        },
    )?;

    // Blocks until the channel receives a value.
    // `allocation_function(usize) -> (buf, buf_len)` is called to reserve enough space by the wasm guest.
    // (buf, buf_len) is returned to the guest. Because Wasmtime's multi-value return is still not there
    // yet, `buf` is returned throug a pointer.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "channel_receive",
        move |mut channel: Option<ExternRef>,
              allocation_function: Option<Func>,
              slice_buf: i32|
              -> i32 {
            let channel = channel.take().unwrap();
            let channel = channel.data();
            if let Some(channel) = channel.downcast_ref::<Channel>() {
                let future = channel.recieve();
                let buffer = env.async_(future).unwrap();
                let iovec = allocation_function
                    .unwrap()
                    .call(&[Val::I32(buffer.len() as i32)]);
                match iovec {
                    Ok(iovec) => {
                        assert_eq!(iovec.len(), 1);
                        let iovec_buf: usize = iovec[0].i32().unwrap() as usize;
                        let iovec_buf_len: usize = buffer.len() as usize;
                        let mut iovec =
                            WasiIoVec::from_values(env.memory(), iovec_buf, iovec_buf_len);
                        buffer.give_to(iovec.as_mut_slice().as_mut_ptr());
                        let mut slice_buf = WasiSize::from(env.memory(), slice_buf as usize);
                        slice_buf.set(iovec_buf as u32);
                        iovec_buf_len as i32
                    }
                    Err(_) => unimplemented!("Trap"),
                }
            } else {
                panic!("Only channels can be sent to")
            }
        },
    )?;

    Ok(())
}
