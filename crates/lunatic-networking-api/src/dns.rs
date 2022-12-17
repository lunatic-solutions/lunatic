use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;
use std::vec::IntoIter;

use anyhow::Result;
use tokio::time::timeout;
use wasmtime::{Caller, Linker};

use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;

use crate::NetworkingCtx;

pub struct DnsIterator {
    iter: IntoIter<SocketAddr>,
}

impl DnsIterator {
    pub fn new(iter: IntoIter<SocketAddr>) -> Self {
        Self { iter }
    }
}

impl Iterator for DnsIterator {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

// Register DNS networking APIs to the linker
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
    Ok(())
}

// Performs a DNS resolution. The returned iterator may not actually yield any values
// depending on the outcome of any resolution performed.
//
// If timeout is specified (value different from `u64::MAX`), the function will return on timeout
// expiration with value 9027.
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
    timeout_duration: u64,
    id_u64_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

        let buffer = memory_slice
            .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
            .or_trap("lunatic::network::resolve")?;
        let name = std::str::from_utf8(buffer)
            .or_trap("lunatic::network::resolve::not_valid_utf8_string")?;

        // Check for timeout during lookup
        let lookup_host = tokio::net::lookup_host(name);
        let (iter_or_error_id, result) = if let Ok(result) = match timeout_duration {
            // Without timeout
            u64::MAX => Ok(lookup_host.await),
            // With timeout
            t => timeout(Duration::from_millis(t), lookup_host).await,
        } {
            match result {
                Ok(sockets) => {
                    // This is a bug in clippy, this collect is not needless
                    #[allow(clippy::needless_collect)]
                    let id = state.dns_resources_mut().add(DnsIterator::new(
                        sockets.collect::<Vec<SocketAddr>>().into_iter(),
                    ));
                    (id, 0)
                }
                Err(error) => {
                    let error_id = state.error_resources_mut().add(error.into());
                    (error_id, 1)
                }
            }
        } else {
            // Call timed out
            (0, 9027)
        };
        let memory = get_memory(&mut caller)?;
        memory
            .write(
                &mut caller,
                id_u64_ptr as usize,
                &iter_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::networking::resolve")?;
        Ok(result)
    })
}

// Drops the DNS iterator resource..
//
// Traps:
// * If the DNS iterator ID doesn't exist.
fn drop_dns_iterator<T: NetworkingCtx>(mut caller: Caller<T>, dns_iter_id: u64) -> Result<()> {
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
) -> Result<u32> {
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
