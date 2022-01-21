use std::{
    convert::TryInto,
    future::Future,
    io::{Read, Write},
    time::Duration,
};

use anyhow::Result;
use wasmtime::{Caller, FuncType, Linker, Trap, ValType};

use crate::{
    api::{error::IntoTrap, get_memory},
    message::{DataMessage, Message},
    process::Signal,
    state::ProcessState,
};

use super::{link_async2_if_match, link_async3_if_match, link_if_match};

// Register the mailbox APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::message",
        "create_data",
        FuncType::new([ValType::I64, ValType::I64], []),
        create_data,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "write_data",
        FuncType::new([ValType::I32, ValType::I32], [ValType::I32]),
        write_data,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "read_data",
        FuncType::new([ValType::I32, ValType::I32], [ValType::I32]),
        read_data,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "seek_data",
        FuncType::new([ValType::I64], []),
        seek_data,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "get_tag",
        FuncType::new([], [ValType::I64]),
        get_tag,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "data_size",
        FuncType::new([], [ValType::I64]),
        data_size,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "push_process",
        FuncType::new([ValType::I64], [ValType::I64]),
        push_process,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "take_process",
        FuncType::new([ValType::I64], [ValType::I64]),
        take_process,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "push_tcp_stream",
        FuncType::new([ValType::I64], [ValType::I64]),
        push_tcp_stream,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "take_tcp_stream",
        FuncType::new([ValType::I64], [ValType::I64]),
        take_tcp_stream,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "send",
        FuncType::new([ValType::I64], []),
        send,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::message",
        "send_receive_skip_search",
        FuncType::new([ValType::I64, ValType::I32], [ValType::I32]),
        send_receive_skip_search,
        namespace_filter,
    )?;
    link_async3_if_match(
        linker,
        "lunatic::message",
        "receive",
        FuncType::new([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]),
        receive,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "push_udp_socket",
        FuncType::new([ValType::I64], [ValType::I64]),
        push_udp_socket,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::message",
        "take_udp_socket",
        FuncType::new([ValType::I64], [ValType::I64]),
        take_udp_socket,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::message
//%
//% There are two kinds of messages a lunatic process can receive:
//% * **data message** that contains a buffer of raw `u8` data and host side resources.
//% * **signal message**, representing a signal that was turned into a message. By setting a flag,
//%   a process can control if when a link dies the process should die too, or just receive a
//%   signal message notifying it about the link's death.
//%
//% All messages have a `tag` allowing for selective receives. If there are already messages in the
//% receiving queue, they will be first searched for a specific tag and the first match returned.
//% Tags are just `i64` values, and a value of 0 indicates no-tag, meaning that it matches all
//% messages.
//%
//% # Data messages
//%
//% Data messages can be created from inside a process and sent to others.
//%
//% They consists of two parts:
//% * A buffer of raw data
//% * An collection of resources
//%
//% If resources are sent between processes, their ID changes. The resource ID can for example
//% be already taken in the receiving process. So we need a way to communicate the new ID on the
//% receiving end.
//%
//% When the `create_data(tag, capacity)` function is called an empty message is allocated and both
//% parts (buffer and resources) can be modified before it's sent to another process. If a new
//% resource is added to the message, the index inside of the message is returned. This information
//% can be now serialized inside the raw data buffer in some way.
//%
//% E.g. Serializing a structure like this:
//%
//% struct A {
//%     a: String,
//%     b: Process,
//%     c: i32,
//%     d: TcpStream
//% }
//%
//% can be done by creating a new data message with `create_data(tag, capacity)`. `capacity` can
//% be used as a hint to the host to pre-reserve the right buffer size. After a message is created,
//% all the resources can be added to it with `add_*`, in this case the fields `b` & `d`. The
//% returned values will be the indexes inside the message.
//%
//% Now the struct can be serialized for example into something like this:
//%
//% ["Some string" | [resource 0] | i32 value | [resource 1] ]
//%
//% [resource 0] & [resource 1] are just encoded as 0 and 1 u64 values, representing their index
//% in the message. Now the message can be sent to another process with `send`.
//%
//% An important limitation here is that messages can only be worked on one at a time. If we
//% called `create_data` again before sending the message, the current buffer and resources
//% would be dropped.
//%
//% On the receiving side, first the `receive(tag)` function must be called. If `tag` has a value
//% different from 0, the function will only return messages that have the specific `tag`. Once
//% a message is received, we can read from its buffer or extract resources from it.
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

//% lunatic::message::create_data(tag: i64, buffer_capacity: u64)
//%
//% * tag - An identifier that can be used for selective receives. If value is 0, no tag is used.
//% * buffer_capacity - A hint to the message to pre-allocate a large enough buffer for writes.
//%
//% Creates a new data message. This message is intended to be modified by other functions in this
//% namespace. Once `lunatic::message::send` is called it will be sent to another process.
fn create_data(mut caller: Caller<ProcessState>, tag: i64, buffer_capacity: u64) {
    let tag = match tag {
        0 => None,
        tag => Some(tag),
    };
    let message = DataMessage::new(tag, buffer_capacity as usize);
    caller.data_mut().message = Some(Message::Data(message));
}

//% lunatic::message::write_data(data_ptr: u32, data_len: u32) -> u32
//%
//% Writes some data into the message buffer and returns how much data is written in bytes.
//%
//% Traps:
//% * If **data_ptr + data_len** is outside the memory.
//% * If it's called without a data message being inside of the scratch area.
fn write_data(mut caller: Caller<ProcessState>, data_ptr: u32, data_len: u32) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let mut message = caller
        .data_mut()
        .message
        .take()
        .or_trap("lunatic::message::write_data")?;
    let buffer = memory
        .data(&caller)
        .get(data_ptr as usize..(data_ptr as usize + data_len as usize))
        .or_trap("lunatic::message::write_data")?;
    let bytes = match &mut message {
        Message::Data(data) => data.write(buffer).or_trap("lunatic::message::write_data")?,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    // Put message back after writing to it.
    caller.data_mut().message = Some(message);

    Ok(bytes as u32)
}

//% lunatic::message::read_data(data_ptr: u32, data_len: u32) -> u32
//%
//% Reads some data from the message buffer and returns how much data is read in bytes.
//%
//% Traps:
//% * If **data_ptr + data_len** is outside the memory.
//% * If it's called without a data message being inside of the scratch area.
fn read_data(mut caller: Caller<ProcessState>, data_ptr: u32, data_len: u32) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let mut message = caller
        .data_mut()
        .message
        .take()
        .or_trap("lunatic::message::read_data")?;
    let buffer = memory
        .data_mut(&mut caller)
        .get_mut(data_ptr as usize..(data_ptr as usize + data_len as usize))
        .or_trap("lunatic::message::read_data")?;
    let bytes = match &mut message {
        Message::Data(data) => data.read(buffer).or_trap("lunatic::message::read_data")?,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    // Put message back after reading from it.
    caller.data_mut().message = Some(message);

    Ok(bytes as u32)
}

//% lunatic::message::seek_data(index: u64)
//%
//% Moves reading head of the internal message buffer. It's useful if you wish to read the a bit
//% of a message, decide that someone else will handle it, `seek_data(0)` to reset the read
//% position for the new receiver and `send` it to another process.
//%
//% Traps:
//% * If it's called without a data message being inside of the scratch area.
fn seek_data(mut caller: Caller<ProcessState>, index: u64) -> Result<(), Trap> {
    let mut message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::seek_data")?;
    match &mut message {
        Message::Data(data) => data.seek(index as usize),
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(())
}

//% lunatic::message::get_tag() -> i64
//%
//% Returns the message tag or 0 if no tag was set.
//%
//% Traps:
//% * If it's called without a message being inside of the scratch area.
fn get_tag(caller: Caller<ProcessState>) -> Result<i64, Trap> {
    let message = caller
        .data()
        .message
        .as_ref()
        .or_trap("lunatic::message::get_tag")?;
    match message.tag() {
        Some(tag) => Ok(tag),
        None => Ok(0),
    }
}

//% lunatic::message::data_size() -> u64
//%
//% Returns the size in bytes of the message buffer.
//%
//% Traps:
//% * If it's called without a data message being inside of the scratch area.
fn data_size(mut caller: Caller<ProcessState>) -> Result<u64, Trap> {
    let message = caller
        .data_mut()
        .message
        .as_ref()
        .or_trap("lunatic::message::data_size")?;
    let bytes = match message {
        Message::Data(data) => data.size(),
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };

    Ok(bytes as u64)
}

//% lunatic::message::push_process(process_id: u64) -> u64
//%
//% Adds a process resource to the message that is currently in the scratch area and returns
//% the location in the array the process was added to. This will remove the process handle from
//% the current process' resources.
//%
//% Traps:
//% * If process ID doesn't exist
//% * If no data message is in the scratch area.
fn push_process(mut caller: Caller<ProcessState>, process_id: u64) -> Result<u64, Trap> {
    let process = caller
        .data_mut()
        .resources
        .processes
        .remove(process_id)
        .or_trap("lunatic::message::push_process")?;
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::push_process")?;
    let index = match message {
        Message::Data(data) => data.add_process(process) as u64,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(index)
}

//% lunatic::message::take_process(index: u64) -> u64
//%
//% Takes the process handle from the message that is currently in the scratch area by index, puts
//% it into the process' resources and returns the resource ID.
//%
//% Traps:
//% * If index ID doesn't exist or matches the wrong resource (not process).
//% * If no data message is in the scratch area.
fn take_process(mut caller: Caller<ProcessState>, index: u64) -> Result<u64, Trap> {
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::take_process")?;
    let process = match message {
        Message::Data(data) => data
            .take_process(index as usize)
            .or_trap("lunatic::message::take_process")?,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(caller.data_mut().resources.processes.add(process))
}

//% lunatic::message::push_tcp_stream(stream_id: u64) -> u64
//%
//% Adds a tcp stream resource to the message that is currently in the scratch area and returns
//% the new location of it. This will remove the tcp stream from  the current process' resources.
//%
//% Traps:
//% * If TCP stream ID doesn't exist
//% * If no data message is in the scratch area.
fn push_tcp_stream(mut caller: Caller<ProcessState>, stream_id: u64) -> Result<u64, Trap> {
    let stream = caller
        .data_mut()
        .resources
        .tcp_streams
        .remove(stream_id)
        .or_trap("lunatic::message::push_tcp_stream")?;
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::push_tcp_stream")?;
    let index = match message {
        Message::Data(data) => data.add_tcp_stream(stream) as u64,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(index)
}

//% lunatic::message::take_tcp_stream(index: u64) -> u64
//%
//% Takes the tcp stream from the message that is currently in the scratch area by index, puts
//% it into the process' resources and returns the resource ID.
//%
//% Traps:
//% * If index ID doesn't exist or matches the wrong resource (not a tcp stream).
//% * If no data message is in the scratch area.
fn take_tcp_stream(mut caller: Caller<ProcessState>, index: u64) -> Result<u64, Trap> {
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::take_tcp_stream")?;
    let tcp_stream = match message {
        Message::Data(data) => data
            .take_tcp_stream(index as usize)
            .or_trap("lunatic::message::take_tcp_stream")?,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(caller.data_mut().resources.tcp_streams.add(tcp_stream))
}

//% lunatic::message::send(
//%     process_id: u64,
//% )
//%
//% Sends the message to a process. There are no guarantees that the process will ever receive
//% the message.
//%
//% Traps:
//% * If the process ID doesn't exist.
//% * If it's called before creating the next message.
fn send(mut caller: Caller<ProcessState>, process_id: u64) -> Result<(), Trap> {
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
    process.send(Signal::Message(message));
    Ok(())
}

//% lunatic::message::send_receive_skip_search(process_id: u64, timeout: u32) -> u32
//%
//% Returns:
//% * 0    if message arrived.
//% * 9027 if call timed out.
//%
//% Sends the message to a process and waits for a reply, but doesn't look through existing
//% messages in the mailbox queue while waiting. This is an optimization that only makes sense
//% with tagged messages. In a request/reply scenario we can tag the request message with an
//% unique tag and just wait on it specifically.
//%
//% This operation needs to be an atomic host function, if we jumped back into the guest we could
//% miss out on the incoming message before `receive` is called.
//%
//% If timeout is specified (value different from 0), the function will return on timeout
//% expiration with value 9027.
//%
//% Traps:
//% * If the process ID doesn't exist.
//% * If it's called with wrong data in the scratch area.
fn send_receive_skip_search(
    mut caller: Caller<ProcessState>,
    process_id: u64,
    timeout: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let message = caller
            .data_mut()
            .message
            .take()
            .or_trap("lunatic::message::send_receive_skip_search")?;
        let mut _tags = [0; 1];
        let tags = if let Some(tag) = message.tag() {
            _tags = [tag];
            Some(&_tags[..])
        } else {
            None
        };
        let process = caller
            .data()
            .resources
            .processes
            .get(process_id)
            .or_trap("lunatic::message::send_receive_skip_search")?;
        process.send(Signal::Message(message));
        if let Some(message) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            message = caller.data_mut().message_mailbox.pop_skip_search(tags) => Some(message)
        } {
            // Put the message into the scratch area
            caller.data_mut().message = Some(message);
            Ok(0)
        } else {
            Ok(9027)
        }
    })
}

//% lunatic::message::receive(tag_ptr: u32, tag_len: u32, timeout: u32) -> u32
//%
//% Returns:
//% * 0    if it's a data message.
//% * 1    if it's a signal turned into a message.
//% * 9027 if call timed out.
//%
//% Takes the next message out of the queue or blocks until the next message is received if queue
//% is empty.
//%
//% If **tag_len** is a value greater than 0 it will block until a message is received matching any
//% of the supplied tags. **tag_ptr** points to an array containing i64 value encoded as little
//% endian values.
//%
//% If timeout is specified (value different from 0), the function will return on timeout
//% expiration with value 9027.
//%
//% Once the message is received, functions like `lunatic::message::read_data()` can be used to
//% extract data out of it.
//%
//% Traps:
//% * If **tag_ptr + (ciovec_array_len * 8) is outside the memory
fn receive(
    mut caller: Caller<ProcessState>,
    tag_ptr: u32,
    tag_len: u32,
    timeout: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let tags = if tag_len > 0 {
            let memory = get_memory(&mut caller)?;
            let buffer = memory
                .data(&caller)
                .get(tag_ptr as usize..(tag_ptr + tag_len * 8) as usize)
                .or_trap("lunatic::message::receive")?;

            // Gether all tags
            let tags: Vec<i64> = buffer
                .chunks_exact(8)
                .map(|chunk| i64::from_le_bytes(chunk.try_into().expect("works")))
                .collect();
            Some(tags)
        } else {
            None
        };

        if let Some(message) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            message = caller.data_mut().message_mailbox.pop(tags.as_deref()) => Some(message)
        } {
            let result = match message {
                Message::Data(_) => 0,
                Message::Signal(_) => 1,
            };
            // Put the message into the scratch area
            caller.data_mut().message = Some(message);
            Ok(result)
        } else {
            Ok(9027)
        }
    })
}

//% lunatic::message::push_udp_socket(socket_id: u64) -> u64
//%
//% Adds a udp socket resource to the message that is currently in the scratch area and returns
//% the new location of it. This will remove the socket from the current process' resources.
//%
//% Traps:
//% * If UDP socket ID doesn't exist
//% * If no data message is in the scratch area.
fn push_udp_socket(mut caller: Caller<ProcessState>, socket_id: u64) -> Result<u64, Trap> {
    let data = caller.data_mut();
    let socket = data
        .resources
        .udp_sockets
        .remove(socket_id)
        .or_trap("lunatic::message::push_udp_socket")?;
    let message = data
        .message
        .as_mut()
        .or_trap("lunatic::message::push_udp_socket")?;
    let index = match message {
        Message::Data(data) => data.add_udp_socket(socket) as u64,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(index)
}

//% lunatic::message::take_udp_socket(index: u64) -> u64
//%
//% Takes the udp socket from the message that is currently in the scratch area by index, puts
//% it into the process' resources and returns the resource ID.
//%
//% Traps:
//% * If index ID doesn't exist or matches the wrong resource (not a udp socket).
//% * If no data message is in the scratch area.
fn take_udp_socket(mut caller: Caller<ProcessState>, index: u64) -> Result<u64, Trap> {
    let message = caller
        .data_mut()
        .message
        .as_mut()
        .or_trap("lunatic::message::take_udp_socket")?;
    let udp_socket = match message {
        Message::Data(data) => data
            .take_udp_socket(index as usize)
            .or_trap("lunatic::message::take_udp_socket")?,
        Message::Signal(_) => {
            return Err(Trap::new("Unexpected `Message::Signal` in scratch area"))
        }
    };
    Ok(caller.data_mut().resources.udp_sockets.add(udp_socket))
}
