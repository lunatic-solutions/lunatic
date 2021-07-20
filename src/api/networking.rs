use std::future::Future;
use std::io::IoSlice;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use wasmtime::Trap;
use wasmtime::{Caller, Linker};

use crate::api::error::IntoTrap;
use crate::state::DnsIterator;
use crate::{api::get_memory, state::ProcessState};

use super::{link_async2_if_match, link_async3_if_match, link_async4_if_match, link_if_match};

// Register the error APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::networking",
        "socket_address_v4",
        socket_address_v4,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "socket_address_v6",
        socket_address_v6,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_socket_address",
        drop_socket_address,
        namespace_filter,
    )?;
    link_async3_if_match(
        linker,
        "lunatic::networking",
        "resolve",
        resolve,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_dns_iterator",
        drop_dns_iterator,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "resolve_next",
        resolve_next,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::networking",
        "tcp_bind",
        tcp_bind,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_tcp_listener",
        drop_tcp_listener,
        namespace_filter,
    )?;
    link_async3_if_match(
        linker,
        "lunatic::networking",
        "tcp_accept",
        tcp_accept,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::networking",
        "tcp_connect",
        tcp_connect,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_tcp_stream",
        drop_tcp_stream,
        namespace_filter,
    )?;
    link_async4_if_match(
        linker,
        "lunatic::networking",
        "tcp_write_vectored",
        tcp_write_vectored,
        namespace_filter,
    )?;
    link_async4_if_match(
        linker,
        "lunatic::networking",
        "tcp_read",
        tcp_read,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::networking",
        "tcp_flush",
        tcp_flush,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::networking::socket_address_v4(address_ptr: i32, port: i32) -> i64
//%
//% Returns the ID of the socket address.
//%
//% Traps:
//% * If **address_ptr + 4** is outside the memory.
fn socket_address_v4(
    mut caller: Caller<ProcessState>,
    address_ptr: u32,
    port: u32,
) -> Result<u64, Trap> {
    let mut buffer = [0; 4];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, address_ptr as usize, &mut buffer)
        .or_trap("lunatic::networking::socket_address_v4")?;
    let socket_addr = SocketAddr::new(IpAddr::from(buffer), port as u16);
    let socket_addr_id = caller
        .data_mut()
        .resources
        .socket_addresses
        .add(socket_addr);
    Ok(socket_addr_id)
}

//% lunatic::networking::socket_address_v6(address_ptr: i32, port: i32) -> i64
//%
//% Returns the ID of the socket address.
//%
//% Traps:
//% * If **address_ptr + 16** is outside the memory.
fn socket_address_v6(
    mut caller: Caller<ProcessState>,
    address_ptr: u32,
    port: u32,
) -> Result<u64, Trap> {
    let mut buffer = [0; 16];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, address_ptr as usize, &mut buffer)
        .or_trap("lunatic::networking::socket_address_v6")?;
    let socket_addr = SocketAddr::new(IpAddr::from(buffer), port as u16);
    let socket_addr_id = caller
        .data_mut()
        .resources
        .socket_addresses
        .add(socket_addr);
    Ok(socket_addr_id)
}

//% lunatic::error::drop_socket_address(socket_addr_id: i64)
//%
//% Drops the socket address resource.
//%
//% Traps:
//% * If the socket address ID doesn't exist.
fn drop_socket_address(mut caller: Caller<ProcessState>, socket_addr_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .socket_addresses
        .remove(socket_addr_id)
        .or_trap("lunatic::networking::drop_socket_address")?;
    Ok(())
}

//% lunatic::networking::resolve(name_str_ptr: i32, name_str_len: i32, id_ptr: i32) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created DNS iterator is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Performs a DNS resolution. The returned iterator may not actually yield any values
//% depending on the outcome of any resolution performed.
//%
//% Traps:
//% * If the name is not a valid utf8 string.
//% * If **name_str_ptr + name_str_len** is outside the memory.
//% * If **id_ptr** is outside the memory.
fn resolve(
    mut caller: Caller<ProcessState>,
    name_str_ptr: u32,
    name_str_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let mut buffer = vec![0; name_str_len as usize];
        let memory = get_memory(&mut caller)?;
        memory
            .read(&caller, name_str_ptr as usize, buffer.as_mut_slice())
            .or_trap("lunatic::network::resolve")?;
        let name = std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::network::resolve")?;
        let (iter_or_error_id, result) = match tokio::net::lookup_host(name).await {
            Ok(iter) => {
                let vec: Vec<SocketAddr> = iter.collect();
                let id = caller
                    .data_mut()
                    .resources
                    .dns_iterators
                    .add(DnsIterator::new(vec.into_iter()));
                (id, 0)
            }
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };
        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &iter_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::spawn")?;
        Ok(result)
    })
}

//% lunatic::error::drop_dns_iterator(dns_iter_id: i64)
//%
//% Drops the DNS iterator resource..
//%
//% Traps:
//% * If the DNS iterator ID doesn't exist.
fn drop_dns_iterator(mut caller: Caller<ProcessState>, dns_iter_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .dns_iterators
        .remove(dns_iter_id)
        .or_trap("lunatic::networking::drop_dns_iterator")?;
    Ok(())
}

//% lunatic::networking::resolve_next(dns_iter_id: i64, id_ptr: i64) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created socket address is written to **id_ptr**
//% * 1 on error   - There are no more addresses in this iterator
//%
//% Takes the next socket address from DNS iterator.
//%
//% Traps:
//% * If the DNS iterator ID doesn't exist.
//% * If **id_ptr** is outside the memory.
fn resolve_next(
    mut caller: Caller<ProcessState>,
    dns_iter_id: u64,
    id_ptr: u32,
) -> Result<u32, Trap> {
    let dns_iter = caller
        .data_mut()
        .resources
        .dns_iterators
        .get_mut(dns_iter_id)
        .or_trap("lunatic::networking::resolve_next")?;

    let (sock_addr_or_error_id, result) = match dns_iter.next() {
        Some(socket_addr) => {
            let sock_addr_id = caller
                .data_mut()
                .resources
                .socket_addresses
                .add(socket_addr);
            (sock_addr_id, 0)
        }
        None => (0, 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(
            &mut caller,
            id_ptr as usize,
            &sock_addr_or_error_id.to_le_bytes(),
        )
        .or_trap("lunatic::networking::resolve_next")?;

    Ok(result)
}

//% lunatic::networking::tcp_bind(socket_addr_id: i64, id_ptr: i32) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP listener is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Creates a new TCP listener, which will be bound to the specified address. The returned listener
//% is ready for accepting connections.
//%
//% Binding with a port number of 0 will request that the OS assigns a port to this listener. The
//% port allocated can be queried via the `local_addr` (TODO) method.
//%
//% Traps:
//% * If the socket address ID doesn't exist.
//% * If **id_ptr** is outside the memory.
fn tcp_bind(
    mut caller: Caller<ProcessState>,
    socket_addr_id: u64,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let socket_addr = caller
            .data()
            .resources
            .socket_addresses
            .get(socket_addr_id)
            .or_trap("lunatic::network::tcp_bind")?;

        let (tcp_listener_or_error_id, result) = match TcpListener::bind(socket_addr).await {
            Ok(listener) => (caller.data_mut().resources.tcp_listeners.add(listener), 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &tcp_listener_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::create_environment")?;

        Ok(result)
    })
}

//% lunatic::error::drop_tcp_listener(tcp_listener_id: i64)
//%
//% Drops the TCP listener resource..
//%
//% Traps:
//% * If the DNS iterator ID doesn't exist.
fn drop_tcp_listener(mut caller: Caller<ProcessState>, tcp_listener_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .tcp_listeners
        .remove(tcp_listener_id)
        .or_trap("lunatic::networking::drop_tcp_listener")?;
    Ok(())
}

//% lunatic::networking::tcp_accept(listener_id: i64, id_ptr: i32, peer_socket_addr_id_ptr: i32) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP stream is written to **id_ptr** and the ID of
//%                  the peer's address is written to ***socket_addr_id_ptr**.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Traps:
//% * If the tcp listener ID doesn't exist.
//% * If **id_ptr** is outside the memory.
//% * If **peer_socket_addr_id_ptr** is outside the memory.
fn tcp_accept(
    mut caller: Caller<ProcessState>,
    listener_id: u64,
    id_ptr: u32,
    socket_addr_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let tcp_listener = caller
            .data()
            .resources
            .tcp_listeners
            .get(listener_id)
            .or_trap("lunatic::network::tcp_accept")?;

        let (tcp_stream_or_error_id, socket_addr_id, result) = match tcp_listener.accept().await {
            Ok((stream, socket_addr)) => (
                caller
                    .data_mut()
                    .resources
                    .tcp_streams
                    .add(Arc::new(Mutex::new(stream))),
                caller
                    .data_mut()
                    .resources
                    .socket_addresses
                    .add(socket_addr),
                0,
            ),
            Err(error) => (caller.data_mut().errors.add(error.into()), 0, 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &tcp_stream_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::tcp_accept")?;
        memory
            .write(
                &mut caller,
                socket_addr_id_ptr as usize,
                &socket_addr_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::tcp_accept")?;
        Ok(result)
    })
}

//% lunatic::networking::tcp_connect(socket_addr_id: i64, id_ptr: i32) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP stream is written to **id_ptr**.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Traps:
//% * If the socket address ID doesn't exist.
//% * If **id_ptr** is outside the memory.
fn tcp_connect(
    mut caller: Caller<ProcessState>,
    socket_addr_id: u64,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let socket_addr = caller
            .data()
            .resources
            .socket_addresses
            .get(socket_addr_id)
            .or_trap("lunatic::network::tcp_connect")?;

        let (stream_or_error_id, result) = match TcpStream::connect(socket_addr).await {
            Ok(stream) => (
                caller
                    .data_mut()
                    .resources
                    .tcp_streams
                    .add(Arc::new(Mutex::new(stream))),
                0,
            ),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &stream_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::tcp_connect")?;
        Ok(result)
    })
}

//% lunatic::error::drop_tcp_stream(tcp_stream_id: i64)
//%
//% Drops the TCP stream resource..
//%
//% Traps:
//% * If the DNS iterator ID doesn't exist.
fn drop_tcp_stream(mut caller: Caller<ProcessState>, tcp_stream_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .tcp_streams
        .remove(tcp_stream_id)
        .or_trap("lunatic::networking::drop_tcp_stream")?;
    Ok(())
}

//% lunatic::networking::tcp_write_vectored(
//%     stream_id: i64,
//%     ciovec_array_ptr: i32,
//%     ciovec_array_len: i32,
//%     i64_opaque_ptr: i32,
//% ) -> i32
//%
//% Returns:
//% * 0 on success - The number of bytes written is written to **opaque_ptr**
//% * 1 on error   - The error ID is written to **opaque_ptr**
//%
//% Gathers data from the vector buffers and writes them to the stream. **ciovec_array_ptr** points
//% to an array of (ciovec_ptr, ciovec_len) pairs where each pair represents a buffer to be written.
//%
//% Traps:
//% * If the stream ID doesn't exist.
//% * If **ciovec_array_ptr + (ciovec_array_len * 4)** is outside the memory, or any of the sub
//%   ciovecs point outside of the memory.
//% * If **i64_opaque_ptr** is outside the memory.
fn tcp_write_vectored(
    mut caller: Caller<ProcessState>,
    stream_id: u64,
    ciovec_array_ptr: u32,
    ciovec_array_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data(&caller)
            .get(ciovec_array_ptr as usize..ciovec_array_len as usize)
            .or_trap("lunatic::networking::tcp_write_vectored")?;

        // Ciovecs consist of 32bit ptr + 32bit len = 8 bytes.
        let vec_slices: Result<Vec<_>> = buffer
            .chunks_exact(8)
            .map(|ciovec| {
                let ciovec_ptr =
                    u32::from_le_bytes([ciovec[0], ciovec[1], ciovec[2], ciovec[3]]) as usize;
                let ciovec_len =
                    u32::from_le_bytes([ciovec[4], ciovec[5], ciovec[6], ciovec[7]]) as usize;
                let slice = memory
                    .data(&caller)
                    .get(ciovec_ptr..ciovec_len)
                    .or_trap("lunatic::networking::tcp_write_vectored")?;
                Ok(IoSlice::new(slice))
            })
            .collect();
        let vec_slices = vec_slices?;

        let stream_mutex = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();
        let mut stream = stream_mutex.lock().await;

        let (opaque, result) = match stream.write_vectored(vec_slices.as_slice()).await {
            Ok(bytes) => (bytes as u64, 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
            .or_trap("lunatic::process::tcp_write_vectored")?;
        Ok(result)
    })
}

//% lunatic::networking::tcp_read(
//%     stream_id: i64,
//%     buffer_ptr: i32,
//%     buffer_len: i32,
//%     i64_opaque_ptr: i32,
//% ) -> i32
//%
//% Returns:
//% * 0 on success - The number of bytes read is written to **opaque_ptr**
//% * 1 on error   - The error ID is written to **opaque_ptr**
//%
//% Reads data from TCP stream and writes it to the buffer.
//%
//% Traps:
//% * If the stream ID doesn't exist.
//% * If **buffer_ptr + buffer_len** is outside the memory.
//% * If **i64_opaque_ptr** is outside the memory.
fn tcp_read(
    mut caller: Caller<ProcessState>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let stream_mutex = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();
        let mut stream = stream_mutex.lock().await;

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..buffer_len as usize)
            .or_trap("lunatic::networking::tcp_read")?;
        let (opaque, result) = match stream.read(buffer).await {
            Ok(bytes) => (bytes as u64, 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
            .or_trap("lunatic::process::tcp_write_vectored")?;
        Ok(result)
    })
}

//% lunatic::networking::tcp_flush(stream_id: i64, error_id_ptr: i32) -> i32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - The error ID is written to **error_id_ptr**
//%
//% Flushes this output stream, ensuring that all intermediately buffered contents reach their
//% destination.
//%
//% Traps:
//% * If the stream ID doesn't exist.
//% * If **error_id_ptr** is outside the memory.
fn tcp_flush(
    mut caller: Caller<ProcessState>,
    stream_id: u64,
    error_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let stream_mutex = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();
        let mut stream = stream_mutex.lock().await;

        let (error_id, result) = match stream.flush().await {
            Ok(()) => (0, 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, error_id_ptr as usize, &error_id.to_le_bytes())
            .or_trap("lunatic::process::tcp_write_vectored")?;
        Ok(result)
    })
}
