use std::{
    convert::TryInto,
    future::Future,
    io::{Read, Write},
};

use anyhow::{anyhow, Result};
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_networking_api::NetworkingCtx;
use lunatic_process_api::ProcessCtx;
use tokio::time::{timeout, Duration};
use wasmtime::{Caller, Linker};

use lunatic_process::{
    message::{DataMessage, Message},
    state::ProcessState,
    Signal,
};

// Register the mailbox APIs to the linker
pub fn register<T: ProcessState + ProcessCtx<T> + NetworkingCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap("lunatic::message", "create_data", create_data)?;
    linker.func_wrap("lunatic::message", "write_data", write_data)?;
    linker.func_wrap("lunatic::message", "read_data", read_data)?;
    linker.func_wrap("lunatic::message", "seek_data", seek_data)?;
    linker.func_wrap("lunatic::message", "get_tag", get_tag)?;
    linker.func_wrap("lunatic::message", "data_size", data_size)?;
    linker.func_wrap("lunatic::message", "push_module", push_module)?;
    linker.func_wrap("lunatic::message", "take_module", take_module)?;
    linker.func_wrap("lunatic::message", "push_tcp_stream", push_tcp_stream)?;
    linker.func_wrap("lunatic::message", "take_tcp_stream", take_tcp_stream)?;
    linker.func_wrap("lunatic::message", "push_tls_stream", push_tls_stream)?;
    linker.func_wrap("lunatic::message", "take_tls_stream", take_tls_stream)?;
    linker.func_wrap("lunatic::message", "send", send)?;
    linker.func_wrap2_async(
        "lunatic::message",
        "send_receive_skip_search",
        send_receive_skip_search,
    )?;
    linker.func_wrap3_async("lunatic::message", "receive", receive)?;
    linker.func_wrap("lunatic::message", "push_udp_socket", push_udp_socket)?;
    linker.func_wrap("lunatic::message", "take_udp_socket", take_udp_socket)?;

    Ok(())
}

// There are two kinds of messages a lunatic process can receive:
//
// 1. **Data message** that contains a buffer of raw `u8` data and host side resources.
// 2. **LinkDied message**, representing a `LinkDied` signal that was turned into a message. The
//    process can control if when a link dies the process should die too, or just receive a
//    `LinkDied` message notifying it about the link's death.
//
// All messages have a `tag` allowing for selective receives. If there are already messages in the
// receiving queue, they will be first searched for a specific tag and the first match returned.
// Tags are just `i64` values, and a value of 0 indicates no-tag, meaning that it matches all
// messages.
//
// # Data messages
//
// Data messages can be created from inside a process and sent to others.
//
// They consists of two parts:
// * A buffer of raw data
// * An collection of resources
//
// If resources are sent between processes, their ID changes. The resource ID can for example
// be already taken in the receiving process. So we need a way to communicate the new ID on the
// receiving end.
//
// When the `create_data(tag, capacity)` function is called an empty message is allocated and both
// parts (buffer and resources) can be modified before it's sent to another process. If a new
// resource is added to the message, the index inside of the message is returned. This information
// can be now serialized inside the raw data buffer in some way.
//
// E.g. Serializing a structure like this:
//
// struct A {
//     a: String,
//     b: Process,
//     c: i32,
//     d: TcpStream
// }
//
// can be done by creating a new data message with `create_data(tag, capacity)`. `capacity` can
// be used as a hint to the host to pre-reserve the right buffer size. After a message is created,
// all the resources can be added to it with `add_*`, in this case the fields `b` & `d`. The
// returned values will be the indexes inside the message.
//
// Now the struct can be serialized for example into something like this:
//
// ["Some string" | [resource 0] | i32 value | [resource 1] ]
//
// [resource 0] & [resource 1] are just encoded as 0 and 1 u64 values, representing their index
// in the message. Now the message can be sent to another process with `send`.
//
// An important limitation here is that messages can only be worked on one at a time. If we
// called `create_data` again before sending the message, the current buffer and resources
// would be dropped.
//
// On the receiving side, first the `receive(tag)` function must be called. If `tag` has a value
// different from 0, the function will only return messages that have the specific `tag`. Once
// a message is received, we can read from its buffer or extract resources from it.
//
// This can be a bit confusing, because resources are just IDs (u64 values) themselves. But we
// still need to serialize them into different u64 values. Resources are inherently bound to a
// process and you can't access another resource just by guessing an ID from another process.
// The process of sending them around needs to be explicit.
//
// This API was designed around the idea that most guest languages will use some serialization
// library and turning resources into indexes is a way of serializing. The same is true for
// deserializing them on the receiving side, when an index needs to be turned into an actual
// resource ID.

// Creates a new data message.
//
// This message is intended to be modified by other functions in this namespace. Once
// `lunatic::message::send` is called it will be sent to another process.
//
// Arguments:
// * tag - An identifier that can be used for selective receives. If value is 0, no tag is used.
// * buffer_capacity - A hint to the message to pre-allocate a large enough buffer for writes.
fn create_data<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    tag: i64,
    buffer_capacity: u64,
) {
    let tag = match tag {
        0 => None,
        tag => Some(tag),
    };
    let message = DataMessage::new(tag, buffer_capacity as usize);
    caller
        .data_mut()
        .message_scratch_area()
        .replace(Message::Data(message));
}

// Writes some data into the message buffer and returns how much data is written in bytes.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
// * If it's called without a data message being inside of the scratch area.
fn write_data<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    data_ptr: u32,
    data_len: u32,
) -> Result<u32> {
    let memory = get_memory(&mut caller)?;
    let mut message = caller
        .data_mut()
        .message_scratch_area()
        .take()
        .or_trap("lunatic::message::write_data")?;
    let buffer = memory
        .data(&caller)
        .get(data_ptr as usize..(data_ptr as usize + data_len as usize))
        .or_trap("lunatic::message::write_data")?;
    let bytes = match &mut message {
        Message::Data(data) => data.write(buffer).or_trap("lunatic::message::write_data")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    // Put message back after writing to it.
    caller.data_mut().message_scratch_area().replace(message);

    Ok(bytes as u32)
}

// Reads some data from the message buffer and returns how much data is read in bytes.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
// * If it's called without a data message being inside of the scratch area.
fn read_data<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    data_ptr: u32,
    data_len: u32,
) -> Result<u32> {
    let memory = get_memory(&mut caller)?;
    let mut message = caller
        .data_mut()
        .message_scratch_area()
        .take()
        .or_trap("lunatic::message::read_data")?;
    let buffer = memory
        .data_mut(&mut caller)
        .get_mut(data_ptr as usize..(data_ptr as usize + data_len as usize))
        .or_trap("lunatic::message::read_data")?;
    let bytes = match &mut message {
        Message::Data(data) => data.read(buffer).or_trap("lunatic::message::read_data")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    // Put message back after reading from it.
    caller.data_mut().message_scratch_area().replace(message);

    Ok(bytes as u32)
}

// Moves reading head of the internal message buffer. It's useful if you wish to read the a bit
// of a message, decide that someone else will handle it, `seek_data(0)` to reset the read
// position for the new receiver and `send` it to another process.
//
// Traps:
// * If it's called without a data message being inside of the scratch area.
fn seek_data<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>, index: u64) -> Result<()> {
    let mut message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::seek_data")?;
    match &mut message {
        Message::Data(data) => data.seek(index as usize),
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(())
}

// Returns the message tag or 0 if no tag was set.
//
// Traps:
// * If it's called without a message being inside of the scratch area.
fn get_tag<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>) -> Result<i64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_ref()
        .or_trap("lunatic::message::get_tag")?;
    match message.tag() {
        Some(tag) => Ok(tag),
        None => Ok(0),
    }
}

// Returns the size in bytes of the message buffer.
//
// Traps:
// * If it's called without a data message being inside of the scratch area.
fn data_size<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_ref()
        .or_trap("lunatic::message::data_size")?;
    let bytes = match message {
        Message::Data(data) => data.size(),
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };

    Ok(bytes as u64)
}

// Adds a module resource to the message that is currently in the scratch area and returns
// the new location of it.
//
// Traps:
// * If module ID doesn't exist
// * If no data message is in the scratch area.
fn push_module<T: ProcessState + ProcessCtx<T> + NetworkingCtx + 'static>(
    mut caller: Caller<T>,
    module_id: u64,
) -> Result<u64> {
    let module = caller
        .data()
        .module_resources()
        .get(module_id)
        .or_trap("lunatic::message::push_module")?
        .clone();
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::push_module")?;
    let index = match message {
        Message::Data(data) => data.add_resource(module) as u64,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(index)
}

// Takes the module from the message that is currently in the scratch area by index, puts
// it into the process' resources and returns the resource ID.
//
// Traps:
// * If index ID doesn't exist or matches the wrong resource (not a module).
// * If no data message is in the scratch area.
fn take_module<T: ProcessState + ProcessCtx<T> + NetworkingCtx + 'static>(
    mut caller: Caller<T>,
    index: u64,
) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::take_module")?;
    let module = match message {
        Message::Data(data) => data
            .take_module(index as usize)
            .or_trap("lunatic::message::take_module")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(caller.data_mut().module_resources_mut().add(module))
}

// Adds a tcp stream resource to the message that is currently in the scratch area and returns
// the new location of it. This will remove the tcp stream from  the current process' resources.
//
// Traps:
// * If TCP stream ID doesn't exist
// * If no data message is in the scratch area.
fn push_tcp_stream<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    stream_id: u64,
) -> Result<u64> {
    let stream = caller
        .data_mut()
        .tcp_stream_resources_mut()
        .remove(stream_id)
        .or_trap("lunatic::message::push_tcp_stream")?;
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::push_tcp_stream")?;
    let index = match message {
        Message::Data(data) => data.add_resource(stream) as u64,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(index)
}

// Takes the tcp stream from the message that is currently in the scratch area by index, puts
// it into the process' resources and returns the resource ID.
//
// Traps:
// * If index ID doesn't exist or matches the wrong resource (not a tcp stream).
// * If no data message is in the scratch area.
fn take_tcp_stream<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    index: u64,
) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::take_tcp_stream")?;
    let tcp_stream = match message {
        Message::Data(data) => data
            .take_tcp_stream(index as usize)
            .or_trap("lunatic::message::take_tcp_stream")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(caller.data_mut().tcp_stream_resources_mut().add(tcp_stream))
}

// move tls stream

// Adds a tls stream resource to the message that is currently in the scratch area and returns
// the new location of it. This will remove the tls stream from  the current process' resources.
//
// Traps:
// * If TLS stream ID doesn't exist
// * If no data message is in the scratch area.
fn push_tls_stream<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    stream_id: u64,
) -> Result<u64> {
    let resources = caller.data_mut().tls_stream_resources_mut();
    let stream = resources
        .remove(stream_id)
        .or_trap("lunatic::message::push_tls_stream")?;
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::push_tls_stream")?;
    let index = match message {
        Message::Data(data) => data.add_resource(stream) as u64,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(index)
}

// Takes the tls stream from the message that is currently in the scratch area by index, puts
// it into the process' resources and returns the resource ID.
//
// Traps:
// * If index ID doesn't exist or matches the wrong resource (not a tls stream).
// * If no data message is in the scratch area.
fn take_tls_stream<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    index: u64,
) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::take_tls_stream")?;
    let tls_stream = match message {
        Message::Data(data) => data
            .take_tls_stream(index as usize)
            .or_trap("lunatic::message::take_tls_stream")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(caller.data_mut().tls_stream_resources_mut().add(tls_stream))
}

// Sends the message to a process.
//
// There are no guarantees that the message will be received.
//
// Traps:
// * If the process ID doesn't exist.
// * If it's called before creating the next message.
fn send<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>, process_id: u64) -> Result<u32> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .take()
        .or_trap("lunatic::message::send::no_message")?;

    if let Some(process) = caller.data_mut().environment().get_process(process_id) {
        process.send(Signal::Message(message));
    }

    Ok(0)
}

// Sends the message to a process and waits for a reply, but doesn't look through existing
// messages in the mailbox queue while waiting. This is an optimization that only makes sense
// with tagged messages. In a request/reply scenario we can tag the request message with an
// unique tag and just wait on it specifically.
//
// This operation needs to be an atomic host function, if we jumped back into the guest we could
// miss out on the incoming message before `receive` is called.
//
// If timeout is specified (value different from `u64::MAX`), the function will return on timeout
// expiration with value 9027.
//
// Returns:
// * 0    if message arrived.
// * 9027 if call timed out.
//
// Traps:
// * If the process ID doesn't exist.
// * If it's called with wrong data in the scratch area.
fn send_receive_skip_search<T: ProcessState + ProcessCtx<T> + Send>(
    mut caller: Caller<T>,
    process_id: u64,
    timeout_duration: u64,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let message = caller
            .data_mut()
            .message_scratch_area()
            .take()
            .or_trap("lunatic::message::send_receive_skip_search")?;
        let mut _tags = [0; 1];
        let tags = if let Some(tag) = message.tag() {
            _tags = [tag];
            Some(&_tags[..])
        } else {
            None
        };

        if let Some(process) = caller.data_mut().environment().get_process(process_id) {
            process.send(Signal::Message(message));
        }

        let pop_skip_search_tag = caller.data_mut().mailbox().pop_skip_search(tags);
        if let Ok(message) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(pop_skip_search_tag.await),
            // With timeout
            t => timeout(Duration::from_millis(t), pop_skip_search_tag).await,
        } {
            // Put the message into the scratch area
            caller.data_mut().message_scratch_area().replace(message);
            Ok(0)
        } else {
            Ok(9027)
        }
    })
}

// Takes the next message out of the queue or blocks until the next message is received if queue
// is empty.
//
// If **tag_len** is a value greater than 0 it will block until a message is received matching any
// of the supplied tags. **tag_ptr** points to an array containing i64 value encoded as little
// endian values.
//
// If timeout is specified (value different from `u64::MAX`), the function will return on timeout
// expiration with value 9027.
//
// Once the message is received, functions like `lunatic::message::read_data()` can be used to
// extract data out of it.
//
// Returns:
// * 0    if it's a data message.
// * 1    if it's a signal turned into a message.
// * 9027 if call timed out.
//
// Traps:
// * If **tag_ptr + (ciovec_array_len * 8) is outside the memory
fn receive<T: ProcessState + ProcessCtx<T> + Send>(
    mut caller: Caller<T>,
    tag_ptr: u32,
    tag_len: u32,
    timeout_duration: u64,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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

        let pop = caller.data_mut().mailbox().pop(tags.as_deref());
        if let Ok(message) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(pop.await),
            // With timeout
            t => timeout(Duration::from_millis(t), pop).await,
        } {
            let result = match message {
                Message::Data(_) => 0,
                Message::LinkDied(_) => 1,
            };
            // Put the message into the scratch area
            caller.data_mut().message_scratch_area().replace(message);
            Ok(result)
        } else {
            Ok(9027)
        }
    })
}

// Adds a udp socket resource to the message that is currently in the scratch area and returns
// the new location of it. This will remove the socket from the current process' resources.
//
// Traps:
// * If UDP socket ID doesn't exist
// * If no data message is in the scratch area.
fn push_udp_socket<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    socket_id: u64,
) -> Result<u64> {
    let data = caller.data_mut();
    let socket = data
        .udp_resources_mut()
        .remove(socket_id)
        .or_trap("lunatic::message::push_udp_socket")?;
    let message = data
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::push_udp_socket")?;
    let index = match message {
        Message::Data(data) => data.add_resource(socket) as u64,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(index)
}

// Takes the udp socket from the message that is currently in the scratch area by index, puts
// it into the process' resources and returns the resource ID.
//
// Traps:
// * If index ID doesn't exist or matches the wrong resource (not a udp socket).
// * If no data message is in the scratch area.
fn take_udp_socket<T: ProcessState + ProcessCtx<T> + NetworkingCtx>(
    mut caller: Caller<T>,
    index: u64,
) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .as_mut()
        .or_trap("lunatic::message::take_udp_socket")?;
    let udp_socket = match message {
        Message::Data(data) => data
            .take_udp_socket(index as usize)
            .or_trap("lunatic::message::take_udp_socket")?,
        Message::LinkDied(_) => {
            return Err(anyhow!("Unexpected `Message::LinkDied` in scratch area"))
        }
    };
    Ok(caller.data_mut().udp_resources_mut().add(udp_socket))
}
