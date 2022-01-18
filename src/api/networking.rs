use std::convert::TryInto;
use std::future::Future;
use std::io::IoSlice;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;
use std::sync::Arc;

use anyhow::Result;
use async_std::io::{ReadExt, WriteExt};
use async_std::net::{TcpListener, TcpStream, UdpSocket};
use wasmtime::{Caller, FuncType, Linker, ValType};
use wasmtime::{Memory, Trap};

use crate::api::error::IntoTrap;
use crate::state::DnsIterator;
use crate::{api::get_memory, state::ProcessState};

use super::{
    link_async2_if_match, link_async3_if_match, link_async4_if_match, link_async5_if_match,
    link_async6_if_match, link_async7_if_match, link_if_match,
};

// Register the error APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_async4_if_match(
        linker,
        "lunatic::networking",
        "resolve",
        FuncType::new(
            [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            [ValType::I32],
        ),
        resolve,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_dns_iterator",
        FuncType::new([ValType::I64], []),
        drop_dns_iterator,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "resolve_next",
        FuncType::new(
            [
                ValType::I64,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        resolve_next,
        namespace_filter,
    )?;
    link_async6_if_match(
        linker,
        "lunatic::networking",
        "tcp_bind",
        FuncType::new(
            [
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        tcp_bind,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_tcp_listener",
        FuncType::new([ValType::I64], []),
        drop_tcp_listener,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "local_addr",
        FuncType::new([ValType::I64, ValType::I32], [ValType::I32]),
        local_addr,
        namespace_filter,
    )?;
    link_async3_if_match(
        linker,
        "lunatic::networking",
        "tcp_accept",
        FuncType::new([ValType::I64, ValType::I32, ValType::I32], [ValType::I32]),
        tcp_accept,
        namespace_filter,
    )?;
    link_async7_if_match(
        linker,
        "lunatic::networking",
        "tcp_connect",
        FuncType::new(
            [
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        tcp_connect,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_tcp_stream",
        FuncType::new([ValType::I64], []),
        drop_tcp_stream,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "clone_tcp_stream",
        FuncType::new([ValType::I64], [ValType::I64]),
        clone_tcp_stream,
        namespace_filter,
    )?;
    link_async5_if_match(
        linker,
        "lunatic::networking",
        "tcp_write_vectored",
        FuncType::new(
            [
                ValType::I64,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        tcp_write_vectored,
        namespace_filter,
    )?;
    link_async5_if_match(
        linker,
        "lunatic::networking",
        "tcp_read",
        FuncType::new(
            [
                ValType::I64,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        tcp_read,
        namespace_filter,
    )?;
    link_async2_if_match(
        linker,
        "lunatic::networking",
        "tcp_flush",
        FuncType::new([ValType::I64, ValType::I32], [ValType::I32]),
        tcp_flush,
        namespace_filter,
    )?;
    link_async6_if_match(
        linker,
        "lunatic::networking",
        "udp_bind",
        FuncType::new(
            [
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        udp_bind,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "drop_udp_socket",
        FuncType::new([ValType::I64], []),
        drop_udp_socket,
        namespace_filter,
    )?;
    link_async6_if_match(
        linker,
        "lunatic::networking",
        "udp_read",
        FuncType::new(
            [
                ValType::I64,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        udp_read,
        namespace_filter,
    )?;
    link_async7_if_match(
        linker,
        "lunatic::networking",
        "udp_connect",
        FuncType::new(
            [
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I32,
            ],
            [ValType::I32],
        ),
        udp_connect,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::networking",
        "clone_udp_socket",
        FuncType::new([ValType::I64], [ValType::I64]),
        clone_udp_socket,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::networking::resolve(
//%     name_str_ptr: u32,
//%     name_str_len: u32,
//%     timeout: u32,
//%     id_u64_ptr: u32,
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created DNS iterator is written to **id_u64_ptr**
//% * 1 on error   - The error ID is written to **id_u64_ptr**
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
    timeout: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let mut buffer = vec![0; name_str_len as usize];
        let memory = get_memory(&mut caller)?;
        memory
            .read(&caller, name_str_ptr as usize, buffer.as_mut_slice())
            .or_trap("lunatic::network::resolve")?;
        let name = std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::network::resolve")?;
        // Check for timeout during lookup
        let return_ = if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = async_net::resolve(name) => Some(result)
        } {
            let (iter_or_error_id, result) = match result {
                Ok(sockets) => {
                    // This is a bug in clippy, this collect is not needless
                    #[allow(clippy::needless_collect)]
                    let id = caller
                        .data_mut()
                        .resources
                        .dns_iterators
                        .add(DnsIterator::new(sockets.into_iter()));
                    (id, 0)
                }
                Err(error) => {
                    let error_id = caller.data_mut().errors.add(error.into());
                    (error_id, 1)
                }
            };
            memory
                .write(
                    &mut caller,
                    id_u64_ptr as usize,
                    &iter_or_error_id.to_le_bytes(),
                )
                .or_trap("lunatic::networking::resolve")?;
            Ok(result)
        } else {
            // Call timed out
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Resolve call timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, id_u64_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::resolve")?;
            Ok(1)
        };
        return_
    })
}

//% lunatic::networking::drop_dns_iterator(dns_iter_id: u64)
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

//% lunatic::networking::resolve_next(
//%     dns_iter_id: u64,
//%     addr_type_u32_ptr: u32,
//%     addr_u8_ptr: u32,
//%     port_u16_ptr: u32,
//%     flow_info_u32_ptr: u32,
//%     scope_id_u32_ptr: u32,
//%  ) -> u32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - There are no more addresses in this iterator
//%
//% Takes the next socket address from DNS iterator and writes it to the passed in pointers.
//% Addresses type is going to be a value of `4` or `6`, representing v4 or v6 addresses. The
//% caller needs to reserve enough space at `addr_u8_ptr` for both values to fit in (16 bytes).
//% `flow_info_u32_ptr` & `scope_id_u32_ptr` are only going to be used with version v6.
//%
//% Traps:
//% * If the DNS iterator ID doesn't exist.
//% * If **addr_type_u32_ptr** is outside the memory
//% * If **addr_u8_ptr** is outside the memory
//% * If **port_u16_ptr** is outside the memory
//% * If **flow_info_u32_ptr** is outside the memory
//% * If **scope_id_u32_ptr** is outside the memory
fn resolve_next(
    mut caller: Caller<ProcessState>,
    dns_iter_id: u64,
    addr_type_u32_ptr: u32,
    addr_u8_ptr: u32,
    port_u16_ptr: u32,
    flow_info_u32_ptr: u32,
    scope_id_u32_ptr: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let dns_iter = caller
        .data_mut()
        .resources
        .dns_iterators
        .get_mut(dns_iter_id)
        .or_trap("lunatic::networking::resolve_next")?;

    match dns_iter.next() {
        Some(socket_addr) => {
            match socket_addr {
                SocketAddr::V4(v4) => {
                    memory
                        .write(&mut caller, addr_type_u32_ptr as usize, &4u32.to_le_bytes())
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(&mut caller, addr_u8_ptr as usize, &v4.ip().octets())
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(&mut caller, port_u16_ptr as usize, &v4.port().to_le_bytes())
                        .or_trap("lunatic::networking::resolve_next")?;
                }
                SocketAddr::V6(v6) => {
                    memory
                        .write(&mut caller, addr_type_u32_ptr as usize, &6u32.to_le_bytes())
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(&mut caller, addr_u8_ptr as usize, &v6.ip().octets())
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(&mut caller, port_u16_ptr as usize, &v6.port().to_le_bytes())
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(
                            &mut caller,
                            flow_info_u32_ptr as usize,
                            &v6.flowinfo().to_le_bytes(),
                        )
                        .or_trap("lunatic::networking::resolve_next")?;
                    memory
                        .write(
                            &mut caller,
                            scope_id_u32_ptr as usize,
                            &v6.scope_id().to_le_bytes(),
                        )
                        .or_trap("lunatic::networking::resolve_next")?;
                }
            }
            Ok(0)
        }
        None => Ok(1),
    }
}

//% lunatic::networking::tcp_bind(
//%     addr_type: u32,
//%     addr_u8_ptr: u32,
//%     port: u32,
//%     flow_info: u32,
//%     scope_id: u32,
//%     id_u64_ptr: u32
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP listener is written to **id_u64_ptr**
//% * 1 on error   - The error ID is written to **id_u64_ptr**
//%
//% Creates a new TCP listener, which will be bound to the specified address. The returned listener
//% is ready for accepting connections.
//%
//% Binding with a port number of 0 will request that the OS assigns a port to this listener. The
//% port allocated can be queried via the `local_addr` (TODO) method.
//%
//% Traps:
//% * If **addr_type** is neither 4 or 6.
//% * If **addr_u8_ptr** is outside the memory
//% * If **id_u64_ptr** is outside the memory.
fn tcp_bind(
    mut caller: Caller<ProcessState>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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
            Ok(listener) => (caller.data_mut().resources.tcp_listeners.add(listener), 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
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

//% lunatic::networking::drop_tcp_listener(tcp_listener_id: i64)
//%
//% Drops the TCP listener resource.
//%
//% Traps:
//% * If the TCP listener ID doesn't exist.
fn drop_tcp_listener(mut caller: Caller<ProcessState>, tcp_listener_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .tcp_listeners
        .remove(tcp_listener_id)
        .or_trap("lunatic::networking::drop_tcp_listener")?;
    Ok(())
}

//% lunatic::networking::local_addr(tcp_listener_id: i64, id_u64_ptr: u32) -> i64
//%
//% Returns the local address that this listener is bound to as an DNS iterator with just one
//% element.
//% * 0 on success - The local address that this listener is bound to is returned as an DNS
//%                  iterator with just one element and written to **id_ptr**.
//%
//% * 1 on error   - The error ID is written to **id_u64_ptr**
//%
//% Traps:
//% * If the tcp listener ID doesn't exist.
//% * If **peer_socket_addr_id_ptr** is outside the memory.
fn local_addr(
    mut caller: Caller<ProcessState>,
    tcp_listener_id: u64,
    id_u64_ptr: u32,
) -> Result<u32, Trap> {
    let tcp_listener = caller
        .data()
        .resources
        .tcp_listeners
        .get(tcp_listener_id)
        .or_trap("lunatic::network::local_addr: listener ID doesn't exist")?;
    let (dns_iter_or_error_id, result) = match tcp_listener.local_addr() {
        Ok(socket_addr) => {
            let dns_iter_id = caller
                .data_mut()
                .resources
                .dns_iterators
                .add(DnsIterator::new(vec![socket_addr].into_iter()));
            (dns_iter_id, 0)
        }
        Err(error) => (caller.data_mut().errors.add(error.into()), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(
            &mut caller,
            id_u64_ptr as usize,
            &dns_iter_or_error_id.to_le_bytes(),
        )
        .or_trap("lunatic::network::local_addr")?;

    Ok(result)
}

//% lunatic::networking::tcp_accept(
//%     listener_id: u64,
//%     id_u64_ptr: u32,
//%     peer_addr_dns_iter_id_u64_ptr: u32
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP stream is written to **id_u64_ptr** and the
//%                  peer address is returned as an DNS iterator with just one element and written
//%                  to **peer_addr_dns_iter_id_u64_ptr**.
//% * 1 on error   - The error ID is written to **id_u64_ptr**
//%
//% Traps:
//% * If the tcp listener ID doesn't exist.
//% * If **id_u64_ptr** is outside the memory.
//% * If **peer_socket_addr_id_ptr** is outside the memory.
fn tcp_accept(
    mut caller: Caller<ProcessState>,
    listener_id: u64,
    id_u64_ptr: u32,
    socket_addr_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let tcp_listener = caller
            .data()
            .resources
            .tcp_listeners
            .get(listener_id)
            .or_trap("lunatic::network::tcp_accept")?;

        let (tcp_stream_or_error_id, peer_addr_iter, result) = match tcp_listener.accept().await {
            Ok((stream, socket_addr)) => {
                let stream_id = caller.data_mut().resources.tcp_streams.add(stream);
                let dns_iter_id = caller
                    .data_mut()
                    .resources
                    .dns_iterators
                    .add(DnsIterator::new(vec![socket_addr].into_iter()));
                (stream_id, dns_iter_id, 0)
            }
            Err(error) => (caller.data_mut().errors.add(error.into()), 0, 1),
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

//% lunatic::networking::tcp_connect(
//%     addr_type: u32,
//%     addr_u8_ptr: u32,
//%     port: u32,
//%     flow_info: u32,
//%     scope_id: u32,
//%     timeout: u32,
//%     id_u64_ptr: u32,
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created TCP stream is written to **id_ptr**.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Traps:
//% * If **addr_type** is neither 4 or 6.
//% * If **addr_u8_ptr** is outside the memory
//% * If **id_u64_ptr** is outside the memory.
#[allow(clippy::too_many_arguments)]
fn tcp_connect(
    mut caller: Caller<ProcessState>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    timeout: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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

        if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = TcpStream::connect(socket_addr) => Some(result)
        } {
            let (stream_or_error_id, result) = match result {
                Ok(stream) => (caller.data_mut().resources.tcp_streams.add(stream), 0),
                Err(error) => (caller.data_mut().errors.add(error.into()), 1),
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
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Connect timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, id_u64_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::tcp_connect")?;
            Ok(1)
        }
    })
}

//% lunatic::networking::drop_tcp_stream(tcp_stream_id: u64)
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

//% lunatic::networking::clone_tcp_stream(tcp_stream_id: u64) -> u64
//%
//% Clones a TCP stream returning the ID of the clone.
//%
//% Traps:
//% * If the stream ID doesn't exist.
fn clone_tcp_stream(mut caller: Caller<ProcessState>, tcp_stream_id: u64) -> Result<u64, Trap> {
    let stream = caller
        .data()
        .resources
        .tcp_streams
        .get(tcp_stream_id)
        .or_trap("lunatic::networking::clone_process")?
        .clone();
    let id = caller.data_mut().resources.tcp_streams.add(stream);
    Ok(id)
}

//% lunatic::networking::tcp_write_vectored(
//%     stream_id: u64,
//%     ciovec_array_ptr: u32,
//%     ciovec_array_len: u32,
//%     timeout: u32,
//%     i64_opaque_ptr: u32,
//% ) -> u32
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
//% * If **ciovec_array_ptr + (ciovec_array_len * 8)** is outside the memory, or any of the sub
//%   ciovecs point outside of the memory.
//% * If **i64_opaque_ptr** is outside the memory.
fn tcp_write_vectored(
    mut caller: Caller<ProcessState>,
    stream_id: u64,
    ciovec_array_ptr: u32,
    ciovec_array_len: u32,
    timeout: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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

        let mut stream = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();

        // Check for timeout
        if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.write_vectored(vec_slices.as_slice()) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().errors.add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::tcp_write_vectored")?;
            Ok(return_)
        } else {
            // Call timed out
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Write call timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, opaque_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::tcp_write_vectored")?;
            Ok(1)
        }
    })
}

//% lunatic::networking::tcp_read(
//%     stream_id: u64,
//%     buffer_ptr: u32,
//%     buffer_len: u32,
//%     timeout: u32,
//%     i64_opaque_ptr: u32,
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
    timeout: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let mut stream = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_read")?
            .clone();

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::tcp_read")?;

        // Check for timeout first
        if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.read(buffer) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().errors.add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::tcp_read")?;
            Ok(return_)
        } else {
            // Call timed out
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Read call timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, opaque_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::tcp_read")?;
            Ok(1)
        }
    })
}

//% lunatic::networking::tcp_flush(stream_id: u64, error_id_ptr: u32) -> u32
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
        let mut stream = caller
            .data()
            .resources
            .tcp_streams
            .get(stream_id)
            .or_trap("lunatic::network::tcp_flush")?
            .clone();

        let (error_id, result) = match stream.flush().await {
            Ok(()) => (0, 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, error_id_ptr as usize, &error_id.to_le_bytes())
            .or_trap("lunatic::networking::tcp_flush")?;
        Ok(result)
    })
}

fn socket_address(
    caller: &Caller<ProcessState>,
    memory: &Memory,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
) -> Result<SocketAddr, Trap> {
    Ok(match addr_type {
        4 => {
            let ip = memory
                .data(&caller)
                .get(addr_u8_ptr as usize..(addr_u8_ptr + 4) as usize)
                .or_trap("lunatic::network::socket_address*")?;
            let addr = <Ipv4Addr as From<[u8; 4]>>::from(ip.try_into().expect("exactly 4 bytes"));
            SocketAddrV4::new(addr, port as u16).into()
        }
        6 => {
            let ip = memory
                .data(&caller)
                .get(addr_u8_ptr as usize..(addr_u8_ptr + 16) as usize)
                .or_trap("lunatic::network::socket_address*")?;
            let addr = <Ipv6Addr as From<[u8; 16]>>::from(ip.try_into().expect("exactly 16 bytes"));
            SocketAddrV6::new(addr, port as u16, flow_info, scope_id).into()
        }
        _ => return Err(Trap::new("Unsupported address type in socket_address*")),
    })
}


//% lunatic::networking::udp_bind(
//%     addr_type: u32,
//%     addr_u8_ptr: u32,
//%     port: u32,
//%     flow_info: u32,
//%     scope_id: u32,
//%     id_u64_ptr: u32
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created UDP listener is written to **id_u64_ptr**
//% * 1 on error   - The error ID is written to **id_u64_ptr**
//%
//% Creates a new UDP listener, which will be bound to the specified address. The returned listener
//% is ready for accepting connections.
//%
//% Binding with a port number of 0 will request that the OS assigns a port to this listener. The
//% port allocated can be queried via the `local_addr` (TODO) method.
//%
//% Traps:
//% * If **addr_type** is neither 4 or 6.
//% * If **addr_u8_ptr** is outside the memory
//% * If **id_u64_ptr** is outside the memory.
fn udp_bind(
    mut caller: Caller<ProcessState>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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
        let (udp_listener_or_error_id, result) = match UdpSocket::bind(socket_addr).await {
            Ok(listener) => (caller.data_mut().resources.udp_sockets.add(Arc::new(listener)), 0),
            Err(error) => (caller.data_mut().errors.add(error.into()), 1),
        };
        memory
            .write(
                &mut caller,
                id_u64_ptr as usize,
                &udp_listener_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::udp_bind")?;

        Ok(result)
    })
}

//% lunatic::networking::drop_udp_listener(tcp_listener_id: i64)
//%
//% Drops the UCP listener resource.
//%
//% Traps:
//% * If the UDP listener ID doesn't exist.
fn drop_udp_socket(mut caller: Caller<ProcessState>, udpp_listener_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .udp_sockets
        .remove(udpp_listener_id)
        .or_trap("lunatic::networking::drop_udp_listener")?;
    Ok(())
}


//% lunatic::networking::tcp_read(
//%     stream_id: u64,
//%     buffer_ptr: u32,
//%     buffer_len: u32,
//%     timeout: u32,
//%     i64_opaque_ptr: u32,
//%     i64_dns_iter_ptr: u32,
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
//% * If **i64_dns_iter_ptr** is outside the memory.
fn udp_read(
    mut caller: Caller<ProcessState>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    timeout: u32,
    opaque_ptr: u32,
    dns_iter_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {

        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

        let buffer = memory_slice
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::udp_read")?;

        let socket = state
            .resources
            .udp_sockets
            .get(stream_id)
            .or_trap("lunatic::network::udp_read")?;

        // Check for timeout first
        if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = socket.recv_from(buffer) => Some(result)
        } {
            let (opaque, socket_result, return_) = match result {
                Ok((bytes, socket)) => (bytes as u64, Some(socket), 0),
                Err(error) => (caller.data_mut().errors.add(error.into()), None, 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_read")?;

            if let Some(socket_addr) = socket_result {
                let dns_iter_id = caller
                    .data_mut()
                    .resources
                    .dns_iterators
                    .add(DnsIterator::new(vec![socket_addr].into_iter()));
                memory
                    .write(&mut caller, dns_iter_ptr as usize, &dns_iter_id.to_le_bytes())
                    .or_trap("lunatic::networking::udp_read")?;
            }
            Ok(return_)
        } else {
            // Call timed out
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Read call timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, opaque_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::udp_read")?;
            Ok(1)
        }
    })
}


//% lunatic::networking::udp_connect(
//%     addr_type: u32,
//%     addr_u8_ptr: u32,
//%     port: u32,
//%     flow_info: u32,
//%     scope_id: u32,
//%     timeout: u32,
//%     id_u64_ptr: u32,
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The ID of the newly created UDP listener is written to **id_ptr**.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Traps:
//% * If **addr_type** is neither 4 or 6.
//% * If **addr_u8_ptr** is outside the memory
//% * If **id_u64_ptr** is outside the memory.
#[allow(clippy::too_many_arguments)]
fn udp_connect(
    mut caller: Caller<ProcessState>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    timeout: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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

        if let Some(result) = tokio::select! {
            _ = async_std::task::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = UdpSocket::bind("127.0.0.1:0") => Some(result)
        } {
            let (stream_or_error_id, result) = match result {
                Ok(socket_result) => {
                    match UdpSocket::connect(&socket_result, socket_addr).await {
                        Ok(()) => (caller.data_mut().resources.udp_sockets.add(Arc::new(socket_result)), 0),
                        Err(connect_error) => (caller.data_mut().errors.add(connect_error.into()), 1),
                    }
                },
                Err(error) => (caller.data_mut().errors.add(error.into()), 1),
            };

            memory
                .write(
                    &mut caller,
                    id_u64_ptr as usize,
                    &stream_or_error_id.to_le_bytes(),
                )
                .or_trap("lunatic::networking::udp_connect")?;
            Ok(result)
        } else {
            // Call timed out
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Connect timed out");
            let error_id = caller.data_mut().errors.add(error.into());
            memory
                .write(&mut caller, id_u64_ptr as usize, &error_id.to_le_bytes())
                .or_trap("lunatic::networking::udp_connect")?;
            Ok(1)
        }
    })
}


//% lunatic::networking::clone_tcp_stream(udp_socket_id: u64) -> u64
//%
//% Clones a UDP socket returning the ID of the clone.
//%
//% Traps:
//% * If the stream ID doesn't exist.
fn clone_udp_socket(mut caller: Caller<ProcessState>, udp_socket_id: u64) -> Result<u64, Trap> {
    let stream = caller
        .data()
        .resources
        .udp_sockets
        .get(udp_socket_id)
        .or_trap("lunatic::networking::clone_udp_socket")?
        .clone();
    let id = caller.data_mut().resources.udp_sockets.add(stream);
    Ok(id)
}
