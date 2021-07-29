use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, FuncType, Linker, Trap, ValType};

use crate::{
    api::{error::IntoTrap, get_memory},
    message::Message,
    state::ProcessState,
};

use super::{link_async2_if_match, link_if_match};

// Register the mailbox APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::message",
        "create",
        FuncType::new([], []),
        create,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "set_buffer",
        FuncType::new([ValType::I32, ValType::I32], []),
        set_buffer,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "add_process",
        FuncType::new([ValType::I64], [ValType::I64]),
        add_process,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "add_tcp_stream",
        FuncType::new([ValType::I64], [ValType::I64]),
        add_tcp_stream,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "send",
        FuncType::new([ValType::I64], [ValType::I32]),
        send,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::message",
        "prepare_receive",
        FuncType::new([ValType::I32, ValType::I32], [ValType::I32]),
        prepare_receive,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "receive",
        FuncType::new([ValType::I32, ValType::I32], []),
        receive,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::message
//%
//% A lunatic message consists of 2 parts:
//% * A buffer of raw data
//% * An array of resource IDs
//%
//% If resources are sent between processes, their ID changes. The resource ID can for example
//% be already taken in the receiving process. So we need a way to communicate the new ID on the
//% receiving end.
//%
//% When the `create()` function is called an empty message is allocated and both parts can be
//% modified before it's sent to another process. If a new resource is added to the message, the
//% index inside of the array is returned. This information can be now serialized inside the raw
//% data buffer in some way. E.g. You are serializing a structure like this:
//%
//% struct A {
//%     a: String,
//%     b: Process,
//%     c: i32,
//%     d: TcpStream
//% }
//%
//% Into something like this:
//%
//% ["Some string" | [resource 0] | i32 value | [resource 1] ]
//%
//% [resource 0] & [resource 1] are just encoded as 0 and 1 u64 values, representing their order
//% in the resource array.
//%
//% It's common to use some serialization library that will encode a mixture of raw data and
//% resource indexes into the data buffer.
//%
//% On the receiving side, first the `prepare_receive()` function must be called to receive info
//% on how big the buffer and resource arrays are, so that enough space can be allocated inside
//% the guest.
//%
//% The `receive()` function will do 2 things:
//% * Write the buffer of raw data to the specified location
//% * Give all resources to the new process (with new IDs) and write the IDs to the specified
//%   location in the same order they were added.
//% Now the information from the buffer (with resource indexes) can be used to deserialize the
//% received message into the same structure.
//%
//% This can be a bit confusing, because resources are just IDs (u64 values) themself. But we
//% still need to serialize them into different u64 values. Resources are inherently bound to a
//% process and you can't access another resource just by guessing an ID from another process.
//% The process of sending them around needs to be explicit.
//%
//% This API was designed around the idea that most guest languages will use some serialization
//% library and turning resources into indexes is a way of serializing. The same is true for
//% deserializing them on the receiving side, when an index needs to be turned into an actual
//% resource ID.

//% lunatic::message::create()
//%
//% Creates a new message. This message is intended to be modified by other functions in this
//% namespace. Once `lunatic::message::send` is called it will be sent to another process.
fn create(mut caller: Caller<ProcessState>) {
    caller.data_mut().message = Some(Message::default());
}

//% lunatic::message::set_buffer(
//%     data_ptr: i32,
//%     data_len: i32,
//% )
//%
//% Sets the data for the next message.
//%
//% Traps:
//% * If **data_ptr + data_len** is outside the memory.
//% * If it's called before the next message is created.
fn set_buffer(mut caller: Caller<ProcessState>, data_ptr: u32, data_len: u32) -> Result<(), Trap> {
    let mut buffer = vec![0; data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, data_ptr as usize, buffer.as_mut_slice())
        .or_trap("lunatic::message::set_buffer")?;
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::set_buffer")?;
    match message {
        Message::Data(data) => data.set_buffer(buffer),
        Message::Signal => return Err(Trap::new("Unexpected `Message::Signal` in scratch buffer")),
    };
    Ok(())
}

//% lunatic::message::add_process(process_id: u64) -> u64
//%
//% Adds a process resource to the next message and returns the location in the array the process
//% was added to. This will remove the process handle from the current process' resources.
//%
//% Traps:
//% * If process ID doesn't exist
//% * If it's called before the next message is created.
fn add_process(mut caller: Caller<ProcessState>, process_id: u64) -> Result<u64, Trap> {
    let process = caller
        .data_mut()
        .resources
        .processes
        .remove(process_id)
        .or_trap("lunatic::message::add_process")?;
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::add_process")?;
    let pid = match message {
        Message::Data(data) => data.add_process(process) as u64,
        Message::Signal => return Err(Trap::new("Unexpected `Message::Signal` in scratch buffer")),
    };
    Ok(pid)
}

//% lunatic::message::add_tcp_stream(stream_id: u64) -> u64
//%
//% Adds a TCP stream resource to the next message and returns the location in the array the TCP
//% stream was added to. This will remove the TCP stream from the current process' resources.
//%
//% Traps:
//% * If TCP stream ID doesn't exist
//% * If it's called before the next message is created.
fn add_tcp_stream(mut caller: Caller<ProcessState>, stream_id: u64) -> Result<u64, Trap> {
    let stream = caller
        .data_mut()
        .resources
        .tcp_streams
        .remove(stream_id)
        .or_trap("lunatic::message::add_tcp_stream")?;
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::add_tcp_stream")?;
    let stream_id = match message {
        Message::Data(data) => data.add_tcp_stream(stream) as u64,
        Message::Signal => return Err(Trap::new("Unexpected `Message::Signal` in scratch buffer")),
    };
    Ok(stream_id)
}

//% lunatic::message::send(
//%     process_id: i64,
//% ) -> i32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - Process can't receive messages (finished).
//%
//% Sends the message to a process.
//%
//% Traps:
//% * If the process ID doesn't exist.
//% * If it's called before a creating the next message.
fn send(mut caller: Caller<ProcessState>, process_id: u64) -> Result<u32, Trap> {
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

//% lunatic::message::prepare_receive(i32_data_size_ptr: i32, i32_res_size_ptr: i32) -> i32
//%
//% Returns:
//% * 0 if it's a regular message.
//% * 1 if it's a signal turned into a message.
//%
//% For regular messages both parameters are used.
//% * **i32_data_size_ptr** - Location to write the message buffer size to as.
//% * **i32_res_size_ptr**  - Location to write the number of resources to.
//%
//% This function should be called before `lunatic::message::receive` to let the guest know how
//% much memory space needs to be reserved for the next message. The data size is in **bytes**,
//% the resources size is the number of resources and each resource is a u64 value. Because of
//% this the guest needs to reserve `64 * resource size` bytes for the resource buffer.
//%
//% Traps:
//% * If **size_ptr** is outside the memory.
fn prepare_receive(
    mut caller: Caller<ProcessState>,
    data_size_ptr: u32,
    res_size_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let message = caller
            .data_mut()
            .mailbox
            .recv()
            .await
            .expect("a process always hold onto its sender and this can't be None");
        let result = match &message {
            Message::Data(message) => {
                let message_buffer_size = message.buffer_size() as u32;
                let message_resources_size = message.resources_size() as u32;
                let memory = get_memory(&mut caller)?;
                memory
                    .write(
                        &mut caller,
                        data_size_ptr as usize,
                        &message_buffer_size.to_le_bytes(),
                    )
                    .or_trap("lunatic::message::prepare_receive")?;
                memory
                    .write(
                        &mut caller,
                        res_size_ptr as usize,
                        &message_resources_size.to_le_bytes(),
                    )
                    .or_trap("lunatic::message::prepare_receive")?;
                0
            }
            Message::Signal => 1,
        };
        // Put the message into the scratch area
        caller.data_mut().message = Some(message);
        Ok(result)
    })
}

//% lunatic::message::receive(data_ptr: i32, resource_ptr: i32)
//%
//% * **data_ptr**     - Pointer to write the data to.
//% * **resource_ptr** - Pointer to an array of i64 values, where each value represents the
//%                      resource id inside the new process. Resources are in the same order they
//%                      were added.
//%
//% Writes the message that was prepared with `lunatic::message::prepare_receive` to the guest. It
//% should only be called if `prepare_receive` returned 0, otherwise it will trap. Signal message
//% don't cary any additional information and everything we need was returned by `prepare_receive`.
//%
//% Traps:
//% * If `lunatic::message::prepare_receive` was not called before.
//% * If **data_ptr + size of the message** is outside the memory.
//% * If **resource_ptr + size of the resources** is outside the memory.
fn receive(mut caller: Caller<ProcessState>, data_ptr: u32, resource_ptr: u32) -> Result<(), Trap> {
    let last_message = caller
        .data_mut()
        .message
        .take()
        .or_trap("lunatic::message::receive")?;
    match last_message {
        Message::Data(last_message) => {
            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, data_ptr as usize, last_message.buffer())
                .or_trap("lunatic::message::receive")?;
            let resources: Vec<u8> = last_message
                .resources()
                .into_iter()
                .map(|resource| match resource {
                    crate::message::Resource::Process(process_handle) => {
                        u64::to_le_bytes(caller.data_mut().resources.processes.add(process_handle))
                    }
                    crate::message::Resource::TcpStream(tcp_stream) => {
                        u64::to_le_bytes(caller.data_mut().resources.tcp_streams.add(tcp_stream))
                    }
                })
                .flatten()
                .collect();
            memory
                .write(&mut caller, resource_ptr as usize, &resources)
                .or_trap("lunatic::message::receive")?;
            Ok(())
        }
        Message::Signal => Err(Trap::new("`lunatic::message::receive` called on a signal")),
    }
}
