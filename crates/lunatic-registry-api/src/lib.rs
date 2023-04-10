use std::{collections::HashMap, future::Future, mem::transmute};

use anyhow::{anyhow, Result};
use lunatic_common_api::{get_memory, IntoTrap, MetricsExt};
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessCtx;
use once_cell::sync::OnceCell;
use opentelemetry::{
    global,
    metrics::{Counter, Meter, Unit, UpDownCounter},
};
use tokio::sync::RwLockWriteGuard;
use wasmtime::{Caller, Linker};

struct Metrics {
    _meter: Meter,
    registered: UpDownCounter<i64>,
    read: Counter<u64>,
    write: Counter<u64>,
    delete: Counter<u64>,
}

static METRICS: OnceCell<Metrics> = OnceCell::new();

// Register the registry APIs to the linker
pub fn register<T: ProcessState + ProcessCtx<T> + Send + Sync + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.registry");

        let registered = meter
            .i64_up_down_counter("registered")
            .with_unit(Unit::new("count"))
            .with_description("Number or processes currently registered")
            .init();
        let read = meter
            .u64_counter("read")
            .with_unit(Unit::new("count"))
            .with_description("Number of entries read from the registry")
            .init();
        let write = meter
            .u64_counter("write")
            .with_unit(Unit::new("count"))
            .with_description("Number of entries written to the registry")
            .init();
        let delete = meter
            .u64_counter("delete")
            .with_unit(Unit::new("count"))
            .with_description("Number of entries deleted from the registry")
            .init();

        Metrics {
            _meter: meter,
            registered,
            read,
            write,
            delete,
        }
    });

    linker.func_wrap4_async("lunatic::registry", "put", put)?;
    linker.func_wrap4_async("lunatic::registry", "get", get)?;
    linker.func_wrap4_async("lunatic::registry", "get_or_put_later", get_or_put_later)?;
    linker.func_wrap2_async("lunatic::registry", "remove", remove)?;

    Ok(())
}

// Registers process with ID under `name`.
//
// Traps:
// * If the process ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn put<T: ProcessState + ProcessCtx<T> + Send + Sync>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    node_id: u64,
    process_id: u64,
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
        let name = memory_slice
            .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
            .or_trap("lunatic::registry::put")?;
        let name = std::str::from_utf8(name).or_trap("lunatic::registry::put")?;

        match state.registry_atomic_put().take() {
            // Use existing lock for writing.
            Some(mut registry_lock) => {
                registry_lock.insert(name.to_owned(), (node_id, process_id));
            }
            // If no lock exists, acquire it.
            None => {
                state
                    .registry()
                    .write()
                    .await
                    .insert(name.to_owned(), (node_id, process_id));
            }
        }

        METRICS.with_current_context(|metrics, cx| {
            metrics.registered.add(&cx, 1, &[]);
            metrics.write.add(&cx, 1, &[]);
        });

        Ok(())
    })
}

// Looks up process under `name` and returns 0 if it was found or 1 if not found.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn get<T: ProcessState + ProcessCtx<T> + Send + Sync>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    node_id_ptr: u32,
    process_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
        let name = memory_slice
            .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
            .or_trap("lunatic::registry::get")?;
        let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

        // Sanity check
        if state.registry_atomic_put().is_some() {
            return Err(anyhow!(
                "calling `lunatic::registry::get` after `get_or_put_later` will deadlock"
            ));
        }

        METRICS.with_current_context(|metrics, cx| {
            metrics.read.add(&cx, 1, &[]);
        });

        let (node_id, process_id) = if let Some(process) = state.registry().read().await.get(name) {
            *process
        } else {
            return Ok(1);
        };

        memory
            .write(&mut caller, node_id_ptr as usize, &node_id.to_le_bytes())
            .or_trap("lunatic::registry::get")?;

        memory
            .write(
                &mut caller,
                process_id_ptr as usize,
                &process_id.to_le_bytes(),
            )
            .or_trap("lunatic::registry::get")?;
        Ok(0)
    })
}

// Looks up a process under `name` and returns 0 if it was found or 1 if not found.
//
// This is intended to be used as part of an atomic lookup in combination with `put`.
// If this function returns `1`, the guest **is required** to call `put` to register
// a process. If this is not done, the registry will be locked forever for **every**
// process. This behavior is necessary to make this registry operation atomic.
//
// TODO: To make guest implementations more reliable, I believe we should move this
// functionality into the `spawn` function. In that case we don't need to keep the
// lock when going back to the guest. Right now we lock, and then the guest is in
// charge to spawn a new process and calling `put` for it.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn get_or_put_later<T: ProcessState + ProcessCtx<T> + Send + Sync>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    node_id_ptr: u32,
    process_id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
        let name = memory_slice
            .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
            .or_trap("lunatic::registry::get")?;
        let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

        METRICS.with_current_context(|metrics, cx| {
            metrics.read.add(&cx, 1, &[]);
        });

        // Lock the registry for every other process before lookup, to make sure
        // nobody else can insert anything before us.
        let registry_lock = state.registry().write().await;
        // Extend the lifetime of the lock, so it can be saved across host calls.
        // Safety:
        //   The process state (containing the lock) can't outlive the `RwLock`,
        //   which is global. This makes it safe to extend the lifetime.
        let registry_lock: RwLockWriteGuard<'static, HashMap<String, (u64, u64)>> =
            unsafe { transmute(registry_lock) };

        if let Some(process) = registry_lock.get(name) {
            let (node_id, process_id) = *process;

            memory
                .write(&mut caller, node_id_ptr as usize, &node_id.to_le_bytes())
                .or_trap("lunatic::registry::get")?;

            memory
                .write(
                    &mut caller,
                    process_id_ptr as usize,
                    &process_id.to_le_bytes(),
                )
                .or_trap("lunatic::registry::get")?;
            Ok(0)
        } else {
            // Save the lock for the next `put` call if no process under this name exists.
            *state.registry_atomic_put() = Some(registry_lock);
            Ok(1)
        }
    })
}

// Removes process under `name` if it exists.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn remove<T: ProcessState + ProcessCtx<T> + Send + Sync>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
        let name = memory_slice
            .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
            .or_trap("lunatic::registry::get")?;
        let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

        // Sanity check
        if state.registry_atomic_put().is_some() {
            return Err(anyhow!(
                "calling `lunatic::registry::remove` after `get_or_put_later` will deadlock"
            ));
        }

        state.registry().write().await.remove(name);

        METRICS.with_current_context(|metrics, cx| {
            metrics.delete.add(&cx, 1, &[]);
            metrics.registered.add(&cx, -1, &[]);
        });

        Ok(())
    })
}
