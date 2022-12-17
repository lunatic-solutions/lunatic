use std::convert::TryInto;
use std::future::Future;
use std::io::IoSlice;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::time::timeout;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use wasmtime::{Caller, Linker};

use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;

use crate::dns::DnsIterator;
use crate::{socket_address, NetworkingCtx, TcpConnection};

// Register TCP networking APIs to the linker
pub fn register<T: NetworkingCtx + ErrorCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap6_async("lunatic::networking", "tcp_bind", tcp_bind)?;
    linker.func_wrap(
        "lunatic::networking",
        "drop_tcp_listener",
        drop_tcp_listener,
    )?;
    linker.func_wrap("lunatic::networking", "tcp_local_addr", tcp_local_addr)?;
    linker.func_wrap3_async("lunatic::networking", "tcp_accept", tcp_accept)?;
    linker.func_wrap7_async("lunatic::networking", "tcp_connect", tcp_connect)?;
    linker.func_wrap("lunatic::networking", "drop_tcp_stream", drop_tcp_stream)?;
    linker.func_wrap("lunatic::networking", "clone_tcp_stream", clone_tcp_stream)?;
    linker.func_wrap4_async(
        "lunatic::networking",
        "tcp_write_vectored",
        tcp_write_vectored,
    )?;
    linker.func_wrap4_async("lunatic::networking", "tcp_peek", tcp_peek)?;
    linker.func_wrap4_async("lunatic::networking", "tcp_read", tcp_read)?;
    linker.func_wrap2_async("lunatic::networking", "set_read_timeout", set_read_timeout)?;
    linker.func_wrap2_async(
        "lunatic::networking",
        "set_write_timeout",
        set_write_timeout,
    )?;
    linker.func_wrap2_async("lunatic::networking", "set_peek_timeout", set_peek_timeout)?;
    linker.func_wrap1_async("lunatic::networking", "get_read_timeout", get_read_timeout)?;
    linker.func_wrap1_async(
        "lunatic::networking",
        "get_write_timeout",
        get_write_timeout,
    )?;
    linker.func_wrap1_async("lunatic::networking", "get_peek_timeout", get_peek_timeout)?;
    linker.func_wrap2_async("lunatic::networking", "tcp_flush", tcp_flush)?;
    Ok(())
}

// Creates a new TCP listener, which will be bound to the specified address. The returned listener
// is ready for accepting connections.
//
// Binding with a port number of 0 will request that the OS assigns a port to this listener. The
// port allocated can be queried via the `tcp_local_addr` (TODO) method.
//
// Returns:
// * 0 on success - The ID of the newly created TCP listener is written to **id_u64_ptr**
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn tcp_bind<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let socket_addr = socket_address(
            &caller,
            &memory,
            addr_type,
            addr_u8_ptr,
            port,
            flow_info,
            scope_id,
        )?;
        let (tcp_listener_or_error_id, result) = match TcpListener::bind(socket_addr).await {
            Ok(listener) => (
                caller.data_mut().tcp_listener_resources_mut().add(listener),
                0,
            ),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };
        memory
            .write(
                &mut caller,
                id_u64_ptr as usize,
                &tcp_listener_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::create_environment")?;

        Ok(result)
    })
}

// Drops the TCP listener resource.
//
// Traps:
// * If the TCP listener ID doesn't exist.
fn drop_tcp_listener<T: NetworkingCtx>(mut caller: Caller<T>, tcp_listener_id: u64) -> Result<()> {
    caller
        .data_mut()
        .tcp_listener_resources_mut()
        .remove(tcp_listener_id)
        .or_trap("lunatic::networking::drop_tcp_listener")?;
    Ok(())
}

// Returns the local address that this listener is bound to as an DNS iterator with just one
// element.
// * 0 on success - The local address that this listener is bound to is returned as an DNS
//                  iterator with just one element and written to **id_ptr**.
//
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If the tcp listener ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_local_addr<T: NetworkingCtx + ErrorCtx>(
    mut caller: Caller<T>,
    tcp_listener_id: u64,
    id_u64_ptr: u32,
) -> Result<u32> {
    let tcp_listener = caller
        .data()
        .tcp_listener_resources()
        .get(tcp_listener_id)
        .or_trap("lunatic::network::tcp_local_addr: listener ID doesn't exist")?;
    let (dns_iter_or_error_id, result) = match tcp_listener.local_addr() {
        Ok(socket_addr) => {
            let dns_iter_id = caller
                .data_mut()
                .dns_resources_mut()
                .add(DnsIterator::new(vec![socket_addr].into_iter()));
            (dns_iter_id, 0)
        }
        Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(
            &mut caller,
            id_u64_ptr as usize,
            &dns_iter_or_error_id.to_le_bytes(),
        )
        .or_trap("lunatic::network::tcp_local_addr")?;

    Ok(result)
}

// Returns:
// * 0 on success - The ID of the newly created TCP stream is written to **id_u64_ptr** and the
//                  peer address is returned as an DNS iterator with just one element and written
//                  to **peer_addr_dns_iter_id_u64_ptr**.
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If the tcp listener ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_accept<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    listener_id: u64,
    id_u64_ptr: u32,
    socket_addr_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let tcp_listener = caller
            .data()
            .tcp_listener_resources()
            .get(listener_id)
            .or_trap("lunatic::network::tcp_accept")?;

        let (tcp_stream_or_error_id, peer_addr_iter, result) = match tcp_listener.accept().await {
            Ok((stream, socket_addr)) => {
                let stream_id = caller
                    .data_mut()
                    .tcp_stream_resources_mut()
                    .add(Arc::new(TcpConnection::new(stream)));
                let dns_iter_id = caller
                    .data_mut()
                    .dns_resources_mut()
                    .add(DnsIterator::new(vec![socket_addr].into_iter()));
                (stream_id, dns_iter_id, 0)
            }
            Err(error) => (
                caller.data_mut().error_resources_mut().add(error.into()),
                0,
                1,
            ),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                id_u64_ptr as usize,
                &tcp_stream_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::tcp_accept")?;
        memory
            .write(
                &mut caller,
                socket_addr_id_ptr as usize,
                &peer_addr_iter.to_le_bytes(),
            )
            .or_trap("lunatic::networking::tcp_accept")?;
        Ok(result)
    })
}

// If timeout is specified (value different from `u64::MAX`), the function will return on timeout
// expiration with value 9027.
//
// Returns:
// * 0 on success - The ID of the newly created TCP stream is written to **id_ptr**.
// * 1 on error   - The error ID is written to **id_ptr**
// * 9027 if the operation timed out
//
// Traps:
// * If **addr_type** is neither 4 or 6.
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn tcp_connect<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    timeout_duration: u64,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let socket_addr = socket_address(
            &caller,
            &memory,
            addr_type,
            addr_u8_ptr,
            port,
            flow_info,
            scope_id,
        )?;

        let connect = TcpStream::connect(socket_addr);
        if let Ok(result) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(connect.await),
            // With timeout
            t => timeout(Duration::from_millis(t), connect).await,
        } {
            let (stream_or_error_id, result) = match result {
                Ok(stream) => (
                    caller
                        .data_mut()
                        .tcp_stream_resources_mut()
                        .add(Arc::new(TcpConnection::new(stream))),
                    0,
                ),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            memory
                .write(
                    &mut caller,
                    id_u64_ptr as usize,
                    &stream_or_error_id.to_le_bytes(),
                )
                .or_trap("lunatic::networking::tcp_connect")?;
            Ok(result)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Drops the TCP stream resource..
//
// Traps:
// * If the DNS iterator ID doesn't exist.
fn drop_tcp_stream<T: NetworkingCtx>(mut caller: Caller<T>, tcp_stream_id: u64) -> Result<()> {
    caller
        .data_mut()
        .tcp_stream_resources_mut()
        .remove(tcp_stream_id)
        .or_trap("lunatic::networking::drop_tcp_stream")?;
    Ok(())
}

// Clones a TCP stream returning the ID of the clone.
//
// Traps:
// * If the stream ID doesn't exist.
fn clone_tcp_stream<T: NetworkingCtx>(mut caller: Caller<T>, tcp_stream_id: u64) -> Result<u64> {
    let stream = caller
        .data()
        .tcp_stream_resources()
        .get(tcp_stream_id)
        .or_trap("lunatic::networking::clone_process")?
        .clone();
    let id = caller.data_mut().tcp_stream_resources_mut().add(stream);
    Ok(id)
}

// Gathers data from the vector buffers and writes them to the stream. **ciovec_array_ptr** points
// to an array of (ciovec_ptr, ciovec_len) pairs where each pair represents a buffer to be written.
//
// Returns:
// * 0 on success - The number of bytes written is written to **opaque_ptr**
// * 1 on error   - The error ID is written to **opaque_ptr**
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_write_vectored<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    ciovec_array_ptr: u32,
    ciovec_array_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data(&caller)
            .get(ciovec_array_ptr as usize..(ciovec_array_ptr + ciovec_array_len * 8) as usize)
            .or_trap("lunatic::networking::tcp_write_vectored")?;

        // Ciovecs consist of 32bit ptr + 32bit len = 8 bytes.
        let vec_slices: Result<Vec<_>> = buffer
            .chunks_exact(8)
            .map(|ciovec| {
                let ciovec_ptr =
                    u32::from_le_bytes(ciovec[0..4].try_into().expect("works")) as usize;
                let ciovec_len =
                    u32::from_le_bytes(ciovec[4..8].try_into().expect("works")) as usize;
                let slice = memory
                    .data(&caller)
                    .get(ciovec_ptr..(ciovec_ptr + ciovec_len))
                    .or_trap("lunatic::networking::tcp_write_vectored")?;
                Ok(IoSlice::new(slice))
            })
            .collect();
        let vec_slices = vec_slices?;

        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();

        let write_timeout = stream.write_timeout.lock().await;
        let mut stream = stream.writer.lock().await;

        if let Ok(write_result) = match *write_timeout {
            Some(write_timeout) => {
                timeout(write_timeout, stream.write_vectored(vec_slices.as_slice())).await
            }
            None => Ok(stream.write_vectored(vec_slices.as_slice()).await),
        } {
            let (opaque, return_) = match write_result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::tcp_write_vectored")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Sets the new value for write timeout for the **TcpStream**
//
// Returns:
// * 0 on success
//
// Traps:
// * If the stream ID doesn't exist.
fn set_write_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tcp_stream_resources_mut()
            .get_mut(stream_id)
            .or_trap("lunatic::network::set_write_timeout")?
            .clone();
        let mut timeout = stream.write_timeout.lock().await;
        // a way to disable the timeout
        if duration == u64::MAX {
            *timeout = None;
        } else {
            *timeout = Some(Duration::from_millis(duration));
        }
        Ok(())
    })
}

// Gets the value for write timeout for the **TcpStream**
//
// Returns:
// * value of write timeout duration in milliseconds
//
// Traps:
// * If the stream ID doesn't exist.
fn get_write_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::get_write_timeout")?
            .clone();
        let timeout = stream.write_timeout.lock().await;
        // a way to disable the timeout
        Ok(timeout.map_or(u64::MAX, |t| t.as_millis() as u64))
    })
}

// Sets the new value for write timeout for the **TcpStream**
//
// Returns:
// * 0 on success
//
// Traps:
// * If the stream ID doesn't exist.
pub fn set_read_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tcp_stream_resources_mut()
            .get_mut(stream_id)
            .or_trap("lunatic::network::set_read_timeout")?
            .clone();
        let mut timeout = stream.read_timeout.lock().await;
        // a way to disable the timeout
        if duration == u64::MAX {
            *timeout = None;
        } else {
            *timeout = Some(Duration::from_millis(duration));
        }
        Ok(())
    })
}

// Gets the value for read timeout for the **TcpStream**
//
// Returns:
// * value of write timeout duration in milliseconds
//
// Traps:
// * If the stream ID doesn't exist.
fn get_read_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::get_read_timeout")?
            .clone();
        let timeout = stream.read_timeout.lock().await;
        // a way to disable the timeout
        Ok(timeout.map_or(u64::MAX, |t| t.as_millis() as u64))
    })
}

// Sets the new value for write timeout for the **TcpStream**
//
// Returns:
// * 0 on success
//
// Traps:
// * If the stream ID doesn't exist.
pub fn set_peek_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tcp_stream_resources_mut()
            .get_mut(stream_id)
            .or_trap("lunatic::network::set_peek_timeout")?
            .clone();
        let mut timeout = stream.peek_timeout.lock().await;
        // a way to disable the timeout
        if duration == u64::MAX {
            *timeout = None;
        } else {
            *timeout = Some(Duration::from_millis(duration));
        }
        Ok(())
    })
}

// Gets the value for peek timeout for the **TcpStream**
//
// Returns:
// * value of peek timeout duration in milliseconds
//
// Traps:
// * If the stream ID doesn't exist.
fn get_peek_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::get_peek_timeout")?
            .clone();
        let timeout = stream.peek_timeout.lock().await;
        // a way to disable the timeout
        Ok(timeout.map_or(u64::MAX, |t| t.as_millis() as u64))
    })
}

// Reads data from TCP stream and writes it to the buffer.
//
// If no data was read within the specified timeout duration the value 9027 is returned
//
// Returns:
// * 0 on success - The number of bytes read is written to **opaque_ptr**
// * 1 on error   - The error ID is written to **opaque_ptr**
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_read<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_read")?
            .clone();
        let read_timeout = stream.read_timeout.lock().await;
        let mut stream = stream.reader.lock().await;

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::tcp_read")?;

        if let Ok(read_result) = match *read_timeout {
            Some(read_timeout) => timeout(read_timeout, stream.read(buffer)).await,
            None => Ok(stream.read(buffer).await),
        } {
            let (opaque, return_) = match read_result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::tcp_read")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Reads data from TCP stream and writes it to the buffer, however does not remove it from the
// internal buffer and therefore will be readable again on the next `peek()` or `read()`
//
// If no data was read within the specified timeout duration the value 9027 is returned
//
// Returns:
// * 0 on success - The number of bytes read is written to **opaque_ptr**
// * 1 on error   - The error ID is written to **opaque_ptr**
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_peek<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_peek")?
            .clone();
        let peek_timeout = stream.peek_timeout.lock().await;
        let mut stream = stream.reader.lock().await;

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::tcp_peek")?;

        if let Ok(read_result) = match *peek_timeout {
            Some(peek_timeout) => timeout(peek_timeout, stream.peek(buffer)).await,
            None => Ok(stream.read(buffer).await),
        } {
            let (opaque, return_) = match read_result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::tcp_peek")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Flushes this output stream, ensuring that all intermediately buffered contents reach their
// destination.
//
// Returns:
// * 0 on success
// * 1 on error   - The error ID is written to **error_id_ptr**
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_flush<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    error_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_flush")?
            .clone();

        let mut stream = stream.writer.lock().await;

        let (error_id, result) = match stream.flush().await {
            Ok(()) => (0, 0),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, error_id_ptr as usize, &error_id.to_le_bytes())
            .or_trap("lunatic::networking::tcp_flush")?;
        Ok(result)
    })
}
