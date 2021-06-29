use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, Linker, Trap};

use crate::{
    api::{error::IntoTrap, get_memory},
    message::Message,
    state::State,
};

use super::{link_async1_if_match, link_if_match};

// Register the mailbox APIs to the linker
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &Vec<String>) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::message",
        "create",
        create,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "set_buffer",
        set_buffer,
        namespace_filter,
    )?;
    link_if_match(linker, "lunatic::message", "send", send, namespace_filter)?;
    link_async1_if_match(
        linker,
        "lunatic::message",
        "prepare_receive",
        prepare_receive,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "receive",
        receive,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::message::create()
//%
//% Creates a new message. This message is intended to be modified by other functions in this
//% namespace. Once `lunatic::message::send` it will be sent to another process and a new one
//% needs to be created.
fn create(mut caller: Caller<State>) {
    caller.data_mut().message = Some(Message::default());
}

//% lunatic::message::set_buffer(
//%     data_ptr: i32,
//%     data_len: i32,
//% ) -> i64
//%
//% Sets the data for the next message.
//%
//% Traps:
//% * If **data_ptr + data_len** is outside the memory.
//% * If it's called before a creating the next message.
fn set_buffer(mut caller: Caller<State>, data_ptr: u32, data_len: u32) -> Result<(), Trap> {
    let mut buffer = vec![0; data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, data_ptr as usize, buffer.as_mut_slice())
        .or_trap("lunatic::message::set_buffer")?;
    caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::set_buffer")?
        .set_buffer(buffer);
    Ok(())
}

//% lunatic::message::send(
//%     process_id: i64,
//% ) -> i64
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - Process can't receive messages (finished).
//%
//% Sends the message to a process
//%
//% Traps:
//% * If the process ID doesn't exist.
//% * If it's called before a creating the next message.
fn send(
    mut caller: Caller<State>,
    process_id: u64,
    data_ptr: u32,
    data_len: u32,
) -> Result<i32, Trap> {
    let mut buffer = vec![0; data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, data_ptr as usize, buffer.as_mut_slice())
        .or_trap("lunatic::message::send")?;
    let message = caller
        .data_mut()
        .message
        .take()
        .or_trap("lunatic::message::send")?;
    let process = caller
        .data()
        .resources
        .processes
        .get(process_id)
        .or_trap("lunatic::message::send")?;
    let result = match process.send_message(message) {
        Ok(()) => 0,
        Err(_error) => 1,
    };
    Ok(result)
}

//% lunatic::message::prepare_receive(i32_size_ptr: i32) -> i64
//%
//% Returns:
//% * 0 on success - The size of the last message is written to **size_ptr**.
//% * 1 on error   - Process can't receive more messages (nobody holds a handle to it).
//%
//% This function should be called before `lunatic::message::receive` to let the guest know how
//% much memory space needs to be reserved for the next message.
//%
//% Traps:
//% * If **size_ptr** is outside the memory.
fn prepare_receive(
    mut caller: Caller<State>,
    size_ptr: u32,
) -> Box<dyn Future<Output = Result<i32, Trap>> + Send + '_> {
    Box::new(async move {
        let message = match caller.data_mut().mailbox.recv().await {
            Some(message) => message,
            None => return Ok(1),
        };

        let message_buffer_size = message.buffer_size() as u32;
        caller.data_mut().last_received_message = Some(message);
        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                size_ptr as usize,
                &message_buffer_size.to_le_bytes(),
            )
            .or_trap("lunatic::message::prepare_receive")?;
        Ok(0)
    })
}

//% lunatic::message::receive(data_ptr: i32)
//%
//% Writes the message that was prepared with `lunatic::message::prepare_receive` to the guest.
//%
//% Traps:
//% * If `lunatic::message::prepare_receive` was not called before.
//% * If **data_ptr + size of the message** is outside the memory.
fn receive(mut caller: Caller<State>, data_ptr: u32) -> Result<(), Trap> {
    let last_message = caller
        .data_mut()
        .last_received_message
        .take()
        .or_trap("lunatic::message::receive")?;

    // TODO: Extract resources

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, data_ptr as usize, last_message.buffer())
        .or_trap("lunatic::message::receive")?;

    Ok(())
}
