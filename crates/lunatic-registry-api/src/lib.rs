use anyhow::Result;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessCtx;
use wasmtime::{Caller, Linker};

// Register the registry APIs to the linker
pub fn register<T: ProcessState + ProcessCtx<T> + 'static>(linker: &mut Linker<T>) -> Result<()> {
    linker.func_wrap("lunatic::registry", "put", put)?;
    linker.func_wrap("lunatic::registry", "get", get)?;
    linker.func_wrap("lunatic::registry", "remove", remove)?;

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.registry.write",
        metrics::Unit::Count,
        "number of new entries written to the registry"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.timers.read",
        metrics::Unit::Count,
        "number of entries read from the registry"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.timers.deletion",
        metrics::Unit::Count,
        "number of entries deleted from the registry"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_gauge!(
        "lunatic.timers.registered",
        metrics::Unit::Count,
        "number of processes currently registered"
    );

    Ok(())
}

// Registers process with ID under `name`.
//
// Traps:
// * If the process ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn put<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    node_id: u64,
    process_id: u64,
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::put")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::put")?;

    state
        .registry()
        .insert(name.to_owned(), (node_id, process_id));
    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.registry.write");

    #[cfg(feature = "metrics")]
    metrics::increment_gauge!("lunatic.registry.registered", 1.0);

    Ok(())
}

// Looks up process under `name` and returns 0 if it was found or 1 if not found.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn get<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    node_id_ptr: u32,
    process_id_ptr: u32,
) -> Result<u32> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::get")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.registry.read");

    let (node_id, process_id) = if let Some(process) = state.registry().get(name) {
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
}

// Removes process under `name` if it exists.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn remove<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::get")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

    state.registry().remove(name);

    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.registry.deletion");

    #[cfg(feature = "metrics")]
    metrics::decrement_gauge!("lunatic.registry.registered", 1.0);

    Ok(())
}
