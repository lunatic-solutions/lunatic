use std::convert::TryInto;
use std::future::Future;
use std::io::{self, IoSlice};
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
use crate::{socket_address, NetworkingCtx, TlsConnection, TlsListener};
use tokio_rustls::rustls::{self, OwnedTrustAnchor};
use tokio_rustls::{webpki, TlsAcceptor, TlsConnector, TlsStream};

// Register TLS networking APIs to the linker
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
    linker.func_wrap7_async("lunatic::networking", "tls_connect", tls_connect)?;
    linker.func_wrap("lunatic::networking", "drop_tls_stream", drop_tls_stream)?;
    linker.func_wrap("lunatic::networking", "clone_tls_stream", clone_tls_stream)?;
    linker.func_wrap4_async(
        "lunatic::networking",
        "tls_write_vectored",
        tls_write_vectored,
    )?;
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
) -> Result<u32> {
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

// Creates a new TLS listener, which will be bound to the specified address. The returned listener
// is ready for accepting connections.
//
// Binding with a port number of 0 will request that the OS assigns a port to this listener. The
// port allocated can be queried via the `tls_local_addr` (TODO) method.
//
// Returns:
// * 0 on success - The ID of the newly created TLS listener is written to **id_u64_ptr**
// * 1 on error   - The error ID is written to **id_u64_ptr**
//
// Traps:
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
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
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let certs = memory
            .data(&caller)
            .get(certs_array_ptr as usize..(certs_array_ptr + certs_array_len) as usize)
            .or_trap("lunatic::networking::tls_bind")?
            .to_vec();

        let keys = memory
            .data(&caller)
            .get(keys_array_ptr as usize..(keys_array_ptr + keys_array_len) as usize)
            .or_trap("lunatic::networking::tls_bind")?
            .to_vec();
        let keys = load_private_key(&keys)
            .or_trap("lunatic::networking::tls_bind::failed to unpack the keys")?;
        let certs = load_certs(&certs)
            .or_trap("lunatic::networking::tls_bind::failed to unpack the certs")?;
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
            .or_trap("lunatic::networking::tls_bind::create_environment")?;

        Ok(result)
    })
}

// Drops the TLS listener resource.
//
// Traps:
// * If the TLS listener ID doesn't exist.
fn drop_tls_listener<T: NetworkingCtx>(mut caller: Caller<T>, tls_listener_id: u64) -> Result<()> {
    caller
        .data_mut()
        .tls_listener_resources_mut()
        .remove(tls_listener_id)
        .or_trap("lunatic::networking::drop_tls_listener")?;
    Ok(())
}

// Returns:
// * 0 on success - The ID of the newly created TLS stream is written to **id_u64_ptr** and the
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
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let tls_listener = caller
            .data()
            .tls_listener_resources()
            .get(listener_id)
            .or_trap("lunatic::network::tls_accept")?;
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
                        .or_trap("lunatic::network::tls_accept server_config")?;
                    let acceptor = TlsAcceptor::from(Arc::new(config));
                    let stream = acceptor
                        .accept(stream)
                        .await
                        .or_trap("unexpected tls error")?;

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
    let mut reader = io::BufReader::new(file);

    // Load and return a single private key.
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
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
// If cert_array_len is 0 it is treated as if there's no cert and the default certs are added
//
// Returns:
// * 0 on success - The ID of the newly created TLS stream is written to **id_ptr**.
// * 1 on error   - The error ID is written to **id_ptr**
// * 9027 if the operation timed out
//
// Traps:
// * If **addr_type** is neither 4 or 6.
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn tls_connect<T: NetworkingCtx + ErrorCtx + Send>(
    mut caller: Caller<T>,
    addr_str_ptr: u32,
    addr_str_len: u32,
    port: u32,
    timeout_duration: u64,
    id_u64_ptr: u32,
    certs_array_ptr: u32,
    certs_array_len: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;

        let socket_addr = String::from_utf8(
            memory
                .data(&caller)
                .get(addr_str_ptr as usize..(addr_str_ptr + addr_str_len) as usize)
                .or_trap("lunatic::networking::tls_connect")?
                .to_vec(),
        )
        .or_trap("lunatic::network::tls_connect::tls_connect_socket_addr")?;

        // if cerst_array_len is 0 this means there are no custom certs
        let cafile = if certs_array_len == 0 {
            None
        } else {
            let certs_list = memory
                .data(&caller)
                .get(certs_array_ptr as usize..(certs_array_ptr + certs_array_len * 8) as usize)
                .or_trap("lunatic::networking::tls_connect")?
                .to_vec();

            let vec_slices: Result<Vec<_>> = certs_list
                .chunks_exact(8)
                .map(|ciovec| {
                    let ciovec_ptr = u32::from_le_bytes(
                        ciovec[0..4]
                            .try_into()
                            .or_trap("lunatic::networking::tls_connect::read_ciovec_ptr")?,
                    ) as usize;
                    let ciovec_len = u32::from_le_bytes(
                        ciovec[4..8]
                            .try_into()
                            .or_trap("lunatic::networking::tls_connect::read_ciovec_len")?,
                    ) as usize;
                    let slice = memory
                        .data(&caller)
                        .get(ciovec_ptr..(ciovec_ptr + ciovec_len))
                        .or_trap("lunatic::networking::tls_connect")?;
                    Ok(slice.to_vec())
                })
                .collect();
            Some(vec_slices)
        };

        let mut root_cert_store = rustls::RootCertStore::empty();
        if let Some(Ok(pem_list)) = cafile {
            let trust_anchors = pem_list
                .iter()
                .map(|pem| {
                    let certs =
                        load_certs(pem).or_trap("lunatic::networking::tls_connect::load_certs")?;
                    let ta = webpki::TrustAnchor::try_from_cert_der(&certs.0[..])
                        .or_trap("lunatic::networking::tls_connect::load_cert DER")?;
                    Ok(OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    ))
                })
                .filter_map(|r: Result<OwnedTrustAnchor>| r.ok());
            root_cert_store.add_server_trust_anchors(trust_anchors);
        } else {
            root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
                |ta| {
                    OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                },
            ));
        }

        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth(); // i guess this was previously the default?

        let connector = TlsConnector::from(Arc::new(config));
        let connect = TcpStream::connect((&socket_addr[..], port as u16));
        if let Ok(result) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(connect.await),
            // With timeout
            t => timeout(Duration::from_millis(t), connect).await,
        } {
            let (stream_or_error_id, result) = match result {
                Ok(stream) => {
                    let domain = &socket_addr[..];
                    let domain = rustls::ServerName::try_from(domain)
                        .or_trap("lunatic::networking::tls_connect::invalid_dnsname")?;

                    let stream = connector
                        .connect(domain, stream)
                        .await
                        .or_trap("lunatic::networking::tls_connect::connect failed")?;
                    (
                        caller
                            .data_mut()
                            .tls_stream_resources_mut()
                            .add(Arc::new(TlsConnection::new(TlsStream::Client(stream)))),
                        0,
                    )
                }
                Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
            };

            memory
                .write(
                    &mut caller,
                    id_u64_ptr as usize,
                    &stream_or_error_id.to_le_bytes(),
                )
                .or_trap("lunatic::networking::tls_connect")?;
            Ok(result)
        } else {
            // Call timed out
            Ok(9027)
        }
    })
}

// Drops the TLS stream resource..
//
// Traps:
// * If the DNS iterator ID doesn't exist.
fn drop_tls_stream<T: NetworkingCtx>(mut caller: Caller<T>, tls_stream_id: u64) -> Result<()> {
    caller
        .data_mut()
        .tls_stream_resources_mut()
        .remove(tls_stream_id)
        .or_trap("lunatic::networking::drop_tls_stream")?;
    Ok(())
}

// Clones a TLS stream returning the ID of the clone.
//
// Traps:
// * If the stream ID doesn't exist.
fn clone_tls_stream<T: NetworkingCtx>(mut caller: Caller<T>, tls_stream_id: u64) -> Result<u64> {
    let stream = caller
        .data()
        .tls_stream_resources()
        .get(tls_stream_id)
        .or_trap("lunatic::networking::clone_tls_stream")?
        .clone();
    let id = caller.data_mut().tls_stream_resources_mut().add(stream);
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
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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
                let ciovec_ptr = u32::from_le_bytes(
                    ciovec[0..4]
                        .try_into()
                        .or_trap("lunatic::network::tls_write_vectored::ciovec_ptr")?,
                ) as usize;
                let ciovec_len = u32::from_le_bytes(
                    ciovec[4..8]
                        .try_into()
                        .or_trap("lunatic::network::tls_write_vectored::ciovec_ptr")?,
                ) as usize;
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

// Sets the new value for write timeout for the **TlsStream**
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
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tls_stream_resources_mut()
            .get_mut(stream_id)
            .or_trap("lunatic::network::set_tls_write_timeout")?
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

// Gets the value for write timeout for the **TlsStream**
//
// Returns:
// * value of write timeout duration in milliseconds
//
// Traps:
// * If the stream ID doesn't exist.
fn get_tls_write_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::get_tls_write_timeout")?
            .clone();
        let timeout = stream.write_timeout.lock().await;
        // a way to disable the timeout
        Ok(timeout.map_or(u64::MAX, |t| t.as_millis() as u64))
    })
}

// Sets the new value for write timeout for the **TlsStream**
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
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data_mut()
            .tls_stream_resources_mut()
            .get_mut(stream_id)
            .or_trap("lunatic::network::set_tls_read_timeout")?
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

// Gets the value for read timeout for the **TlsStream**
//
// Returns:
// * value of write timeout duration in milliseconds
//
// Traps:
// * If the stream ID doesn't exist.
fn get_tls_read_timeout<T: NetworkingCtx + ErrorCtx + Send>(
    caller: Caller<T>,
    stream_id: u64,
) -> Box<dyn Future<Output = Result<u64>> + Send + '_> {
    Box::new(async move {
        let stream = caller
            .data()
            .tls_stream_resources()
            .get(stream_id)
            .or_trap("lunatic::network::get_tls_read_timeout")?
            .clone();
        let timeout = stream.read_timeout.lock().await;
        // a way to disable the timeout
        Ok(timeout.map_or(u64::MAX, |t| t.as_millis() as u64))
    })
}

// Reads data from TLS stream and writes it to the buffer.
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
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
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
