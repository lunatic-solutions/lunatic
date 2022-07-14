pub mod dns;

use std::convert::TryInto;
use std::future::Future;
use std::io::IoSlice;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use dns::DnsIterator;
use hash_map_id::HashMapId;
use lunatic_error_api::ErrorCtx;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
};
use wasmtime::{Caller, Linker};
use wasmtime::{Memory, Trap};

use lunatic_common_api::{get_memory, IntoTrap};

pub struct TcpConnection {
    pub reader: Mutex<OwnedReadHalf>,
    pub writer: Mutex<OwnedWriteHalf>,
}

impl TcpConnection {
    pub fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        TcpConnection {
            reader: Mutex::new(read_half),
            writer: Mutex::new(write_half),
        }
    }
}

pub type TcpListenerResources = HashMapId<TcpListener>;
pub type TcpStreamResources = HashMapId<Arc<TcpConnection>>;
pub type UdpResources = HashMapId<Arc<UdpSocket>>;
pub type DnsResources = HashMapId<DnsIterator>;

pub trait NetworkingCtx {
    fn tcp_listener_resources(&self) -> &TcpListenerResources;
    fn tcp_listener_resources_mut(&mut self) -> &mut TcpListenerResources;
    fn tcp_stream_resources(&self) -> &TcpStreamResources;
    fn tcp_stream_resources_mut(&mut self) -> &mut TcpStreamResources;
    fn udp_resources(&self) -> &UdpResources;
    fn udp_resources_mut(&mut self) -> &mut UdpResources;
    fn dns_resources(&self) -> &DnsResources;
    fn dns_resources_mut(&mut self) -> &mut DnsResources;
}

// Register the error APIs to the linker
pub fn register<T: NetworkingCtx + ErrorCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap4_async("lunatic::networking", "resolve", resolve)?;
    linker.func_wrap(
        "lunatic::networking",
        "drop_dns_iterator",
        drop_dns_iterator,
    )?;
    linker.func_wrap("lunatic::networking", "resolve_next", resolve_next)?;
    linker.func_wrap6_async("lunatic::networking", "tcp_bind", tcp_bind)?;
    linker.func_wrap(
        "lunatic::networking",
        "drop_tcp_listener",
        drop_tcp_listener,
    )?;
    linker.func_wrap("lunatic::networking", "tcp_local_addr", tcp_local_addr)?;
    linker.func_wrap("lunatic::networking", "udp_local_addr", udp_local_addr)?;
    linker.func_wrap3_async("lunatic::networking", "tcp_accept", tcp_accept)?;
    linker.func_wrap7_async("lunatic::networking", "tcp_connect", tcp_connect)?;
    linker.func_wrap("lunatic::networking", "drop_tcp_stream", drop_tcp_stream)?;
    linker.func_wrap("lunatic::networking", "clone_tcp_stream", clone_tcp_stream)?;
    linker.func_wrap5_async(
        "lunatic::networking",
        "tcp_write_vectored",
        tcp_write_vectored,
    )?;
    linker.func_wrap5_async("lunatic::networking", "tcp_read", tcp_read)?;
    linker.func_wrap2_async("lunatic::networking", "tcp_flush", tcp_flush)?;
    linker.func_wrap6_async("lunatic::networking", "udp_bind", udp_bind)?;
    linker.func_wrap("lunatic::networking", "drop_udp_socket", drop_udp_socket)?;
    linker.func_wrap5_async("lunatic::networking", "udp_receive", udp_receive)?;
    linker.func_wrap6_async("lunatic::networking", "udp_receive_from", udp_receive_from)?;
    linker.func_wrap8_async("lunatic::networking", "udp_connect", udp_connect)?;
    linker.func_wrap("lunatic::networking", "clone_udp_socket", clone_udp_socket)?;
    linker.func_wrap(
        "lunatic::networking",
        "set_udp_socket_broadcast",
        set_udp_socket_broadcast,
    )?;
    linker.func_wrap(
        "lunatic::networking",
        "get_udp_socket_broadcast",
        get_udp_socket_broadcast,
    )?;
    linker.func_wrap(
        "lunatic::networking",
        "set_udp_socket_ttl",
        set_udp_socket_ttl,
    )?;
    linker.func_wrap(
        "lunatic::networking",
        "get_udp_socket_ttl",
        get_udp_socket_ttl,
    )?;
    linker.func_wrap10_async("lunatic::networking", "udp_send_to", udp_send_to)?;
    linker.func_wrap5_async("lunatic::networking", "udp_send", udp_send)?;

    Ok(())
}

// Performs a DNS resolution. The returned iterator may not actually yield any values
// depending on the outcome of any resolution performed.
//
// Returns:
// * 0 on success - The ID of the newly created DNS iterator is written to **id_u64_ptr**
// * 1 on error   - The error ID is written to **id_u64_ptr**
// * 9027 if the operation timed out
//
// Traps:
// * If the name is not a valid utf8 string.
// * If any memory outside the guest heap space is referenced.
fn resolve<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
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
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = tokio::net::lookup_host(name) => Some(result)
        } {
            let (iter_or_error_id, result) = match result {
                Ok(sockets) => {
                    // This is a bug in clippy, this collect is not needless
                    #[allow(clippy::needless_collect)]
                    let id = caller.data_mut().dns_resources_mut().add(DnsIterator::new(
                        sockets.collect::<Vec<SocketAddr>>().into_iter(),
                    ));
                    (id, 0)
                }
                Err(error) => {
                    let error_id = caller.data_mut().error_resources_mut().add(error.into());
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
            Ok(9027)
        };
        return_
    })
}

// Drops the DNS iterator resource..
//
// Traps:
// * If the DNS iterator ID doesn't exist.
fn drop_dns_iterator<T: NetworkingCtx>(
    mut caller: Caller<T>,
    dns_iter_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .dns_resources_mut()
        .remove(dns_iter_id)
        .or_trap("lunatic::networking::drop_dns_iterator")?;
    Ok(())
}

// Takes the next socket address from DNS iterator and writes it to the passed in pointers.
//
// Addresses type is going to be a value of `4` or `6`, representing v4 or v6 addresses. The
// caller needs to reserve enough space at `addr_u8_ptr` for both values to fit in (16 bytes).
// `flow_info_u32_ptr` & `scope_id_u32_ptr` are only going to be used with version v6.
//
// Returns:
// * 0 on success
// * 1 on error   - There are no more addresses in this iterator
//
// Traps:
// * If the DNS iterator ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn resolve_next<T: NetworkingCtx>(
    mut caller: Caller<T>,
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
        .dns_resources_mut()
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
fn drop_tcp_listener<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tcp_listener_id: u64,
) -> Result<(), Trap> {
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
) -> Result<u32, Trap> {
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
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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

// Returns:
// * 0 on success - The ID of the newly created TCP stream is written to **id_ptr**.
// * 1 on error   - The error ID is written to **id_ptr**
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
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = TcpStream::connect(socket_addr) => Some(result)
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
fn drop_tcp_stream<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tcp_stream_id: u64,
) -> Result<(), Trap> {
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
fn clone_tcp_stream<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tcp_stream_id: u64,
) -> Result<u64, Trap> {
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

        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_write_vectored")?
            .clone();

        let mut stream = stream.writer.lock().await;

        // Check for timeout
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.write_vectored(vec_slices.as_slice()) => Some(result)
        } {
            let (opaque, return_) = match result {
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

// Reads data from TCP stream and writes it to the buffer.
//
// Returns:
// * 0 on success - The number of bytes read is written to **opaque_ptr**
// * 1 on error   - The error ID is written to **opaque_ptr**
// * 9027 if the operation timed out
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tcp_read<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    timeout: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tcp_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tcp_read")?
            .clone();
        let mut stream = stream.reader.lock().await;

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::tcp_read")?;

        // Check for timeout first
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.read(buffer) => Some(result)
        } {
            let (opaque, return_) = match result {
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
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
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

// Creates a new UDP socket, which will be bound to the specified address. The returned socket
// is ready for receiving messages.
//
// Binding with a port number of 0 will request that the OS assigns a port to this socket. The
// port allocated can be queried via the `udp_local_addr` method.
//
// Returns:
// * 0 on success - The ID of the newly created UDP socket is written to **id_u64_ptr**
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If **addr_type** is neither 4 or 6.
// * If any memory outside the guest heap space is referenced.
fn udp_bind<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
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
            Ok(listener) => (
                caller
                    .data_mut()
                    .udp_resources_mut()
                    .add(Arc::new(listener)),
                0,
            ),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
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

// Drops the UdpSocket resource.
//
// Traps:
// * If the UDP socket ID doesn't exist.
fn drop_udp_socket<T: NetworkingCtx>(
    mut caller: Caller<T>,
    udp_socket_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .udp_resources_mut()
        .remove(udp_socket_id)
        .or_trap("lunatic::networking::drop_udp_socket")?;
    Ok(())
}

// Reads data from the connected udp socket and writes it to the given buffer. This method will
// fail if the socket is not connected.
//
// Returns:
// * 0 on success    - The number of bytes read is written to **opaque_ptr**
// * 1 on error      - The error ID is written to **opaque_ptr**
// * 9027 on timeout - The socket receive timed out.
//
// Traps:
// * If the socket ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_receive<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    timeout: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

        let buffer = memory_slice
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::udp_receive")?;

        let socket = state
            .udp_resources_mut()
            .get(socket_id)
            .or_trap("lunatic::network::udp_receive")?;

        // Check for timeout first
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = socket.recv(buffer) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_receive")?;

            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Receives data from the socket.
//
// Returns:
// * 0 on success    - The number of bytes read is written to **opaque_ptr** and the sender's
//                     address is returned as a DNS iterator through i64_dns_iter_ptr.
// * 1 on error      - The error ID is written to **opaque_ptr**
// * 9027 on timeout - The socket receive timed out.
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_receive_from<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
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
            .or_trap("lunatic::networking::udp_receive_from")?;

        let socket = state
            .udp_resources_mut()
            .get(socket_id)
            .or_trap("lunatic::network::udp_receive_from")?;

        // Check for timeout first
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = socket.recv_from(buffer) => Some(result)
        } {
            let (opaque, socket_result, return_) = match result {
                Ok((bytes, socket)) => (bytes as u64, Some(socket), 0),
                Err(error) => (
                    caller.data_mut().error_resources_mut().add(error.into()),
                    None,
                    1,
                ),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_receive_from")?;

            if let Some(socket_addr) = socket_result {
                let dns_iter_id = caller
                    .data_mut()
                    .dns_resources_mut()
                    .add(DnsIterator::new(vec![socket_addr].into_iter()));
                memory
                    .write(
                        &mut caller,
                        dns_iter_ptr as usize,
                        &dns_iter_id.to_le_bytes(),
                    )
                    .or_trap("lunatic::networking::udp_receive_from")?;
            }
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Connects the UDP socket to a remote address.
//
// When connected, methods `networking::send` and `networking::receive` will use the specified
// address for sending and receiving messages. Additionally, a filter will be applied to
// `networking::receive_from` so that it only receives messages from that same address.
//
// Returns:
// * 0 on success
// * 1 on error      - The error ID is written to **id_ptr**.
// * 9027 on timeout - The socket connect operation timed out.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn udp_connect<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    udp_socket_id: u64,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    timeout: u32,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        // Get the memory and the socket being connected to
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
        let socket = caller
            .data_mut()
            .udp_resources_mut()
            .get(udp_socket_id)
            .or_trap("lunatic::networking::udp_connect")?;

        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = socket.connect(socket_addr) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(()) => (0, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            memory
                .write(&mut caller, id_u64_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_connect")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Clones a UDP socket returning the ID of the clone.
//
// Traps:
// * If the stream ID doesn't exist.
fn clone_udp_socket<T: NetworkingCtx>(
    mut caller: Caller<T>,
    udp_socket_id: u64,
) -> Result<u64, Trap> {
    let stream = caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::networking::clone_udp_socket")?
        .clone();
    let id = caller.data_mut().udp_resources_mut().add(stream);
    Ok(id)
}

// Sets the broadcast state of the UDP socket.
//
// Traps:
// * If the socket ID doesn't exist.
// * If set_broadcast traps.
fn set_udp_socket_broadcast<T: NetworkingCtx>(
    caller: Caller<T>,
    udp_socket_id: u64,
    broadcast: u32,
) -> Result<(), Trap> {
    caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::networking::set_udp_socket_broadcast")?
        .set_broadcast(broadcast > 0)
        .or_trap("lunatic::networking::set_udp_socket_broadcast")?;
    Ok(())
}

// Gets the current broadcast state of the UdpSocket.
//
// Traps:
// * If the socket ID doesn't exist.
// * If broadcast traps.
fn get_udp_socket_broadcast<T: NetworkingCtx>(
    caller: Caller<T>,
    udp_socket_id: u64,
) -> Result<i32, Trap> {
    let socket = caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::networking::get_udp_socket_broadcast")?;

    let result = socket
        .broadcast()
        .or_trap("lunatic::networking::get_udp_socket_broadcast")?;

    Ok(result as i32)
}

// Sets the ttl of the UDP socket. This value sets the time-to-live field that is used in
// every packet sent from this socket.
//
// Traps:
// * If the socket ID doesn't exist.
// * If set_ttl traps.
fn set_udp_socket_ttl<T: NetworkingCtx>(
    caller: Caller<T>,
    udp_socket_id: u64,
    ttl: u32,
) -> Result<(), Trap> {
    caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::networking::set_udp_socket_ttl")?
        .set_ttl(ttl)
        .or_trap("lunatic::networking::set_udp_socket_ttl")?;
    Ok(())
}

// Gets the current ttl value set on the UdpSocket.
//
// Traps:
// * If the socket ID doesn't exist.
// * If ttl() traps.
fn get_udp_socket_ttl<T: NetworkingCtx>(
    caller: Caller<T>,
    udp_socket_id: u64,
) -> Result<u32, Trap> {
    let result = caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::networking::get_udp_socket_ttl")?
        .ttl()
        .or_trap("lunatic::networking::get_udp_socket_ttl")?;

    Ok(result)
}

// Sends data on the socket to the given address.
//
// Returns:
// * 0 on success    - The number of bytes written is written to **opaque_ptr**
// * 1 on error      - The error ID is written to **opaque_ptr**
// * 9027 on timeout - The socket send timed out.
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn udp_send_to<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    timeout: u32,
    opaque_ptr: u32,
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
        let buffer = memory
            .data(&caller)
            .get(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::udp_send_to")?;

        let stream = caller
            .data()
            .udp_resources()
            .get(socket_id)
            .or_trap("lunatic::network::udp_send_to")?
            .clone();

        // Check for timeout
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.send_to(buffer, socket_addr) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_send_to")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Sends data on the socket to the remote address to which it is connected.
//
// The `networking::udp_connect` method will connect this socket to a remote address. This method
// will fail if the socket is not connected.
//
// Returns:
// * 0 on success    - The number of bytes written is written to **opaque_ptr**
// * 1 on error      - The error ID is written to **opaque_ptr**
// * 9027 on timeout - The socket send timed out.
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_send<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    timeout: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;

        let buffer = memory
            .data(&caller)
            .get(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::udp_send")?;

        let stream = caller
            .data()
            .udp_resources()
            .get(socket_id)
            .or_trap("lunatic::network::udp_send")?
            .clone();

        // Check for timeout
        if let Some(result) = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(timeout as u64)), if timeout != 0 => None,
            result = stream.send(buffer) => Some(result)
        } {
            let (opaque, return_) = match result {
                Ok(bytes) => (bytes as u64, 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            let memory = get_memory(&mut caller)?;
            memory
                .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
                .or_trap("lunatic::networking::udp_send")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Returns the local address of this socket, bound to a DNS iterator with just one
// element.
//
// * 0 on success - The local address that this socket is bound to, returned as a DNS
//                  iterator with just one element and written to **id_ptr**.
// * 1 on error   - The error ID is written to **id_u64_ptr**.
//
// Traps:
// * If the udp socket ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_local_addr<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    udp_socket_id: u64,
    id_u64_ptr: u32,
) -> Result<u32, Trap> {
    let udp_socket = caller
        .data()
        .udp_resources()
        .get(udp_socket_id)
        .or_trap("lunatic::network::udp_local_addr: listener ID doesn't exist")?;
    let (dns_iter_or_error_id, result) = match udp_socket.local_addr() {
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
        .or_trap("lunatic::network::udp_local_addr")?;

    Ok(result)
}

fn socket_address<T: NetworkingCtx>(
    caller: &Caller<T>,
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
