use std::convert::TryInto;
use std::fs;
use std::future::Future;
use std::io::{self, IoSlice, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::time::timeout;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use wasmtime::Trap;
use wasmtime::{Caller, Linker};

use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;

use crate::dns::DnsIterator;
use crate::{socket_address, NetworkingCtx, TcpConnection, TlsConnection, TlsListener};
use tokio_rustls::rustls::{self, OwnedTrustAnchor, PrivateKey};
use tokio_rustls::{webpki, TlsAcceptor, TlsConnector};

// Register TCP networking APIs to the linker
pub fn register<T: NetworkingCtx + ErrorCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap10_async("lunatic::networking", "tls_bind", tls_bind)?;
    linker.func_wrap(
        "lunatic::networking",
        "drop_tls_listener",
        drop_tls_listener,
    )?;
    linker.func_wrap("lunatic::networking", "tls_local_addr", tls_local_addr)?;
    linker.func_wrap3_async("lunatic::networking", "tls_accept", tls_accept)?;
    // linker.func_wrap7_async("lunatic::networking", "tls_connect", tls_connect)?;
    linker.func_wrap("lunatic::networking", "drop_tls_stream", drop_tls_stream)?;
    linker.func_wrap("lunatic::networking", "clone_tls_stream", clone_tls_stream)?;
    linker.func_wrap4_async(
        "lunatic::networking",
        "tls_write_vectored",
        tls_write_vectored,
    )?;
    // linker.func_wrap4_async("lunatic::networking", "tls_peek", tls_peek)?;
    linker.func_wrap4_async("lunatic::networking", "tls_read", tls_read)?;
    linker.func_wrap2_async(
        "lunatic::networking",
        "set_tls_read_timeout",
        set_tls_read_timeout,
    )?;
    linker.func_wrap2_async(
        "lunatic::networking",
        "set_tls_write_timeout",
        set_tls_write_timeout,
    )?;
    linker.func_wrap2_async(
        "lunatic::networking",
        "set_tls_peek_timeout",
        set_tls_peek_timeout,
    )?;
    linker.func_wrap1_async(
        "lunatic::networking",
        "get_tls_read_timeout",
        get_tls_read_timeout,
    )?;
    linker.func_wrap1_async(
        "lunatic::networking",
        "get_tls_write_timeout",
        get_tls_write_timeout,
    )?;
    linker.func_wrap1_async(
        "lunatic::networking",
        "get_tls_peek_timeout",
        get_tls_peek_timeout,
    )?;
    linker.func_wrap2_async("lunatic::networking", "tls_flush", tls_flush)?;
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
// * If the tls listener ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tls_local_addr<T: NetworkingCtx + ErrorCtx>(
    mut caller: Caller<T>,
    tls_listener_id: u64,
    id_u64_ptr: u32,
) -> Result<u32, Trap> {
    let tls_listener = caller
        .data()
        .tls_listener_resources()
        .get(tls_listener_id)
        .or_trap("lunatic::network::tls_local_addr: listener ID doesn't exist")?;
    let (dns_iter_or_error_id, result) = match tls_listener.listener.local_addr() {
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
        .or_trap("lunatic::network::tls_local_addr")?;

    Ok(result)
}

// Creates a new TCP listener, which will be bound to the specified address. The returned listener
// is ready for accepting connections.
//
// Binding with a port number of 0 will request that the OS assigns a port to this listener. The
// port allocated can be queried via the `tls_local_addr` (TODO) method.
//
// Returns:
// * 0 on success - The ID of the newly created TCP listener is written to **id_u64_ptr**
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn tls_bind<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
    id_u64_ptr: u32,
    certs_array_ptr: u32,
    certs_array_len: u32,
    keys_array_ptr: u32,
    keys_array_len: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let certs = memory
            .data(&caller)
            .get(certs_array_ptr as usize..(certs_array_ptr + certs_array_len) as usize)
            .or_trap("lunatic::networking::tls_accept")?
            .to_vec();

        let keys = memory
            .data(&caller)
            .get(keys_array_ptr as usize..(keys_array_ptr + keys_array_len) as usize)
            .or_trap("lunatic::networking::tls_accept")?
            .to_vec();
        let keys = load_private_key(&keys).expect("should have unpacked the keys");
        let certs = load_certs(&certs).expect("should have unpacked the certs");
        let socket_addr = socket_address(
            &caller,
            &memory,
            addr_type,
            addr_u8_ptr,
            port,
            flow_info,
            scope_id,
        )?;
        let (tls_listener_or_error_id, result) = match TcpListener::bind(socket_addr).await {
            Ok(listener) => (
                caller
                    .data_mut()
                    .tls_listener_resources_mut()
                    .add(TlsListener {
                        listener,
                        keys,
                        certs,
                    }),
                0,
            ),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };
        memory
            .write(
                &mut caller,
                id_u64_ptr as usize,
                &tls_listener_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::create_environment")?;

        Ok(result)
    })
}

// Drops the TCP listener resource.
//
// Traps:
// * If the TCP listener ID doesn't exist.
fn drop_tls_listener<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tls_listener_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .tls_listener_resources_mut()
        .remove(tls_listener_id)
        .or_trap("lunatic::networking::drop_tls_listener")?;
    Ok(())
}

// Returns:
// * 0 on success - The ID of the newly created TCP stream is written to **id_u64_ptr** and the
//                  peer address is returned as an DNS iterator with just one element and written
//                  to **peer_addr_dns_iter_id_u64_ptr**.
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If the tls listener ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn tls_accept<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    listener_id: u64,
    id_u64_ptr: u32,
    socket_addr_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let tls_listener = caller
            .data()
            .tls_listener_resources()
            .get(listener_id)
            .or_trap("lunatic::network::tls_accept")?;

        // let certs = memory
        //     .data(&caller)
        //     .get(certs_array_ptr as usize..(certs_array_ptr + certs_array_len) as usize)
        //     .or_trap("lunatic::networking::tls_accept")?
        //     .to_vec();

        // let keys = memory
        //     .data(&caller)
        //     .get(keys_array_ptr as usize..(keys_array_ptr + keys_array_len) as usize)
        //     .or_trap("lunatic::networking::tls_accept")?
        //     .to_vec();
        // let keys = load_private_key(&keys).expect("should have unpacked the keys");
        // let certs = load_certs(&certs).expect("should have unpacked the certs");
        let keys = tls_listener.keys.clone();
        let certs = tls_listener.certs.clone();

        let (tls_stream_or_error_id, peer_addr_iter, result) =
            match tls_listener.listener.accept().await {
                Ok((stream, socket_addr)) => {
                    let config = rustls::ServerConfig::builder()
                        .with_safe_defaults()
                        .with_no_client_auth()
                        .with_single_cert(vec![certs], keys)
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
                        .unwrap(); // todo: handle errors here
                    let acceptor = TlsAcceptor::from(Arc::new(config));
                    let stream = acceptor.accept(stream).await.unwrap();
                    // .map_err(|e| {
                    //     println!("ERROR {:?}", e);
                    //     Trap::new("unexpected tls error")
                    // })?;

                    let stream_id = caller.data_mut().tls_stream_resources_mut().add(Arc::new(
                        TlsConnection::new(tokio_rustls::TlsStream::Server(stream)),
                    ));
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
                &tls_stream_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::tls_accept")?;
        memory
            .write(
                &mut caller,
                socket_addr_id_ptr as usize,
                &peer_addr_iter.to_le_bytes(),
            )
            .or_trap("lunatic::networking::tls_accept")?;
        Ok(result)
    })
}

// Load private key from file.
fn load_private_key(file: &[u8]) -> io::Result<rustls::PrivateKey> {
    // Open keyfile.
    // let keyfile = fs::File::open(filename)?;
    // .map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
    // let mut reader = io::BufReader::new(keyfile);
    let mut reader = io::BufReader::new(file);

    // Load and return a single private key.
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
    // .map_err(|_| error("failed to load private key".into()))?;
    if keys.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "expected a single private key",
        ));
    }

    Ok(rustls::PrivateKey(keys[0].clone()))
}

fn load_certs(file: &[u8]) -> io::Result<rustls::Certificate> {
    let mut reader = io::BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)?;
    if certs.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "expected a single private key",
        ));
    }

    Ok(rustls::Certificate(certs[0].clone()))
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
// #[allow(clippy::too_many_arguments)]
// fn tls_connect<T: NetworkingCtx + ErrorCtx + Send>(
//     mut caller: Caller<T>,
//     addr_type: u32,
//     addr_u8_ptr: u32,
//     port: u32,
//     flow_info: u32,
//     scope_id: u32,
//     timeout_duration: u64,
//     id_u64_ptr: u32,
// ) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
//     Box::new(async move {
//         let memory = get_memory(&mut caller)?;
//         let socket_addr = socket_address(
//             &caller,
//             &memory,
//             addr_type,
//             addr_u8_ptr,
//             port,
//             flow_info,
//             scope_id,
//         )?;

//         let connect = TcpStream::connect(socket_addr);
//         if let Ok(result) = match timeout_duration {
//             // Without timeout
//             u64::MAX => Ok(connect.await),
//             // With timeout
//             t => timeout(Duration::from_millis(t), connect).await,
//         } {
//             let (stream_or_error_id, result) = match result {
//                 Ok(stream) => (
//                     caller
//                         .data_mut()
//                         .tls_stream_resources_mut()
//                         .add(Arc::new(TcpConnection::new(stream))),
//                     0,
//                 ),
//                 Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
//             };

//             memory
//                 .write(
//                     &mut caller,
//                     id_u64_ptr as usize,
//                     &stream_or_error_id.to_le_bytes(),
//                 )
//                 .or_trap("lunatic::networking::tls_connect")?;
//             Ok(result)
//         } else {
//             // Call timed out
//             Ok(9027)
//         }
//     })
// }

// Drops the TCP stream resource..
//
// Traps:
// * If the DNS iterator ID doesn't exist.
fn drop_tls_stream<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tls_stream_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .tls_stream_resources_mut()
        .remove(tls_stream_id)
        .or_trap("lunatic::networking::drop_tls_stream")?;
    Ok(())
}

// Clones a TCP stream returning the ID of the clone.
//
// Traps:
// * If the stream ID doesn't exist.
fn clone_tls_stream<T: NetworkingCtx>(
    mut caller: Caller<T>,
    tls_stream_id: u64,
) -> Result<u64, Trap> {
    println!("START CLONING");
    let stream = caller
        .data()
        .tls_stream_resources()
        .get(tls_stream_id)
        .or_trap("lunatic::networking::clone_process")?
        .clone();
    println!("PROCEED CLONING");
    let id = caller.data_mut().tls_stream_resources_mut().add(stream);
    println!("CLONE DONE {:?}", id);
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
fn tls_write_vectored<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    ciovec_array_ptr: u32,
    ciovec_array_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data(&caller)
            .get(ciovec_array_ptr as usize..(ciovec_array_ptr + ciovec_array_len * 8) as usize)
            .or_trap("lunatic::networking::tls_write_vectored")?;

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
                    .or_trap("lunatic::networking::tls_write_vectored")?;
                Ok(IoSlice::new(slice))
            })
            .collect();
        let vec_slices = vec_slices?;

        let stream = caller
            .data()
            .tls_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tls_write_vectored")?
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
                .or_trap("lunatic::networking::tls_write_vectored")?;
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
fn set_tls_write_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<(), Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tls_stream_resources_mut()
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
fn get_tls_write_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
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
pub fn set_tls_read_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<(), Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tls_stream_resources_mut()
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
fn get_tls_read_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
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
pub fn set_tls_peek_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    duration: u64,
) -> Box<dyn Future<Output = Result<(), Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tls_stream_resources_mut()
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
fn get_tls_peek_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
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
fn tls_read<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    buffer_ptr: u32,
    buffer_len: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tls_read")?
            .clone();
        let read_timeout = stream.read_timeout.lock().await;
        let mut stream = stream.reader.lock().await;

        let memory = get_memory(&mut caller)?;
        let buffer = memory
            .data_mut(&mut caller)
            .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
            .or_trap("lunatic::networking::tls_read")?;

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
                .or_trap("lunatic::networking::tls_read")?;
            Ok(return_)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// // Reads data from TLS stream and writes it to the buffer, however does not remove it from the
// // internal buffer and therefore will be readable again on the next `peek()` or `read()`
// //
// // If no data was read within the specified timeout duration the value 9027 is returned
// //
// // Returns:
// // * 0 on success - The number of bytes read is written to **opaque_ptr**
// // * 1 on error   - The error ID is written to **opaque_ptr**
// //
// // Traps:
// // * If the stream ID doesn't exist.
// // * If any memory outside the guest heap space is referenced.
// fn tls_peek<T: NetworkingCtx + ErrorCtx + Send>(
//     mut caller: Caller<T>,
//     stream_id: u64,
//     buffer_ptr: u32,
//     buffer_len: u32,
//     opaque_ptr: u32,
// ) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
//     Box::new(async move {
//         let stream = caller
//             .data()
//             .tls_stream_resources()
//             .get(stream_id)
//             .or_trap("lunatic::network::tls_peek")?
//             .clone();
//         let peek_timeout = stream.peek_timeout.lock().await;
//         let mut stream = stream.reader.lock().await;

//         let memory = get_memory(&mut caller)?;
//         let buffer = memory
//             .data_mut(&mut caller)
//             .get_mut(buffer_ptr as usize..(buffer_ptr + buffer_len) as usize)
//             .or_trap("lunatic::networking::tls_peek")?;

//         if let Ok(read_result) = match *peek_timeout {
//             Some(peek_timeout) => timeout(peek_timeout, stream.peek(buffer)).await,
//             None => Ok(stream.read(buffer).await),
//         } {
//             let (opaque, return_) = match read_result {
//                 Ok(bytes) => (bytes as u64, 0),
//                 Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
//             };

//             let memory = get_memory(&mut caller)?;
//             memory
//                 .write(&mut caller, opaque_ptr as usize, &opaque.to_le_bytes())
//                 .or_trap("lunatic::networking::tls_peek")?;
//             Ok(return_)
//         } else {
//             // Call timed out
//             Ok(9027)
//         }
//     })
// }

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
fn tls_flush<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    stream_id: u64,
    error_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::tls_flush")?
            .clone();

        let mut stream = stream.writer.lock().await;

        let (error_id, result) = match stream.flush().await {
            Ok(()) => (0, 0),
            Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
        };

        let memory = get_memory(&mut caller)?;
        memory
            .write(&mut caller, error_id_ptr as usize, &error_id.to_le_bytes())
            .or_trap("lunatic::networking::tls_flush")?;
        Ok(result)
    })
}

impl TlsConnection {
    // /// Handles events sent to the TlsConnection by mio::Poll
    // fn ready(&mut self, ev: &mio::event::Event) {
    //     assert_eq!(ev.token(), CLIENT);

    //     if ev.is_readable() {
    //         self.do_read();
    //     }

    //     if ev.is_writable() {
    //         self.do_write();
    //     }

    //     if self.is_closed() {
    //         println!("Connection closed");
    //         process::exit(if self.clean_closure { 0 } else { 1 });
    //     }
    // }

    // fn read_source_to_end(&mut self, rd: &mut dyn io::Read) -> io::Result<usize> {
    //     let mut buf = Vec::new();
    //     let len = rd.read_to_end(&mut buf)?;
    //     self.tls_conn.writer().write_all(&buf).unwrap();
    //     Ok(len)
    // }

    /// We're ready to do a read.
    // fn do_read(&mut self) {
    //     // Read TLS data.  This fails if the underlying TCP connection
    //     // is broken.
    //     match self.tls_conn.read_tls(&mut self.socket) {
    //         Err(error) => {
    //             if error.kind() == io::ErrorKind::WouldBlock {
    //                 return;
    //             }
    //             println!("TLS read error: {:?}", error);
    //             self.closing = true;
    //             return;
    //         }

    //         // If we're ready but there's no data: EOF.
    //         Ok(0) => {
    //             println!("EOF");
    //             self.closing = true;
    //             self.clean_closure = true;
    //             return;
    //         }

    //         Ok(_) => {}
    //     };

    //     // Reading some TLS data might have yielded new TLS
    //     // messages to process.  Errors from this indicate
    //     // TLS protocol problems and are fatal.
    //     let io_state = match self.tls_conn.process_new_packets() {
    //         Ok(io_state) => io_state,
    //         Err(err) => {
    //             println!("TLS error: {:?}", err);
    //             self.closing = true;
    //             return;
    //         }
    //     };

    //     // Having read some TLS data, and processed any new messages,
    //     // we might have new plaintext as a result.
    //     //
    //     // Read it and then write it to stdout.
    //     if io_state.plaintext_bytes_to_read() > 0 {
    //         let mut plaintext = Vec::new();
    //         plaintext.resize(io_state.plaintext_bytes_to_read(), 0u8);
    //         self.tls_conn.reader().read_exact(&mut plaintext).unwrap();
    //         io::stdout().write_all(&plaintext).unwrap();
    //     }

    //     // If wethat fails, the peer might have started a clean TLS-level
    //     // session closure.
    //     if io_state.peer_has_closed() {
    //         self.clean_closure = true;
    //         self.closing = true;
    //     }
    // }

    // fn do_write(&mut self) {
    //     self.tls_conn.write_tls(&mut self.socket).unwrap();
    // }

    // /// Registers self as a 'listener' in mio::Registry
    // fn register(&mut self, registry: &mio::Registry) {
    //     let interest = self.event_set();
    //     registry
    //         .register(&mut self.socket, CLIENT, interest)
    //         .unwrap();
    // }

    // /// Reregisters self as a 'listener' in mio::Registry.
    // fn reregister(&mut self, registry: &mio::Registry) {
    //     let interest = self.event_set();
    //     registry
    //         .reregister(&mut self.socket, CLIENT, interest)
    //         .unwrap();
    // }

    // /// Use wants_read/wants_write to register for different mio-level
    // /// IO readiness events.
    // fn event_set(&self) -> mio::Interest {
    //     let rd = self.tls_conn.wants_read();
    //     let wr = self.tls_conn.wants_write();

    //     if rd && wr {
    //         mio::Interest::READABLE | mio::Interest::WRITABLE
    //     } else if wr {
    //         mio::Interest::WRITABLE
    //     } else {
    //         mio::Interest::READABLE
    //     }
    // }

    fn is_closed(&self) -> bool {
        self.closing
    }
}

// impl Write for TlsConnection {
//     fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
//         self.tls_conn.writer().write(bytes)
//     }

//     fn flush(&mut self) -> io::Result<()> {
//         self.tls_conn.writer().flush()
//     }
// }

// impl Read for TlsConnection {
//     fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
//         self.tls_conn.reader().read(bytes)
//     }
// }
