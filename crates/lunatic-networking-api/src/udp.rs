use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use wasmtime::{Caller, Linker};

use crate::dns::DnsIterator;
use crate::{socket_address, NetworkingCtx};
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;

// Register UDP networking APIs to the linker
pub fn register<T: NetworkingCtx + ErrorCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap6_async("lunatic::networking", "udp_bind", udp_bind)?;
    linker.func_wrap("lunatic::networking", "udp_local_addr", udp_local_addr)?;
    linker.func_wrap("lunatic::networking", "drop_udp_socket", drop_udp_socket)?;
    linker.func_wrap4_async("lunatic::networking", "udp_receive", udp_receive)?;
    linker.func_wrap5_async("lunatic::networking", "udp_receive_from", udp_receive_from)?;
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
    linker.func_wrap9_async("lunatic::networking", "udp_send_to", udp_send_to)?;
    linker.func_wrap4_async("lunatic::networking", "udp_send", udp_send)?;
    Ok(())
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
fn drop_udp_socket<T: NetworkingCtx>(mut caller: Caller<T>, udp_socket_id: u64) -> Result<()> {
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
//
// Traps:
// * If the socket ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_receive<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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

        let (opaque, return_) = match socket.recv(buffer).await {
            Ok(bytes) => (bytes as u64, 0),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
            .or_trap("lunatic::networking::udp_receive")?;

        Ok(return_)
    })
}

// Receives data from the socket.
//
// Returns:
// * 0 on success    - The number of bytes read is written to **opaque_ptr** and the sender's
//                     address is returned as a DNS iterator through i64_dns_iter_ptr.
// * 1 on error      - The error ID is written to **opaque_ptr**
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_receive_from<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
    dns_iter_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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

        let (opaque, socket_result, return_) = match socket.recv_from(buffer).await {
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
    timeout_duration: u64,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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

        let connect = socket.connect(socket_addr);
        if let Ok(result) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(connect.await),
            // With timeout
            t => timeout(Duration::from_millis(t), connect).await,
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
fn clone_udp_socket<T: NetworkingCtx>(mut caller: Caller<T>, udp_socket_id: u64) -> Result<u64> {
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
) -> Result<()> {
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
) -> Result<i32> {
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
) -> Result<()> {
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
fn get_udp_socket_ttl<T: NetworkingCtx>(caller: Caller<T>, udp_socket_id: u64) -> Result<u32> {
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
    opaque_ptr: u32,
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

        let (opaque, return_) = match stream.send_to(buffer, socket_addr).await {
            Ok(bytes) => (bytes as u64, 0),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
            .or_trap("lunatic::networking::udp_send_to")?;
        Ok(return_)
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
//
// Traps:
// * If the stream ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn udp_send<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    socket_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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

        let (opaque, return_) = match stream.send(buffer).await {
            Ok(bytes) => (bytes as u64, 0),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
            .or_trap("lunatic::networking::udp_send")?;
        Ok(return_)
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
) -> Result<u32> {
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
