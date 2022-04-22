use anyhow::Result;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessCtx;
use wasmtime::Trap;
use wasmtime::{Caller, Linker};

// Register the error APIs to the linker
pub fn register<T: ProcessState + ProcessCtx<T> + 'static>(linker: &mut Linker<T>) -> Result<()> {
    linker.func_wrap("lunatic::registry", "put", put)?;
    linker.func_wrap("lunatic::registry", "get", get)?;
    linker.func_wrap("lunatic::registry", "remove", remove)?;
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
    process_id: u64,
) -> Result<(), Trap> {
    let process = caller
        .data_mut()
        .process_resources_mut()
        .get(process_id)
        .or_trap("lunatic::registry::put")?
        .clone();

    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::put")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::put")?;

    state.registry().insert(name.to_owned(), process);

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
    process_id_ptr: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::get")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

    let process = if let Some(process) = state.registry().get(name) {
        process.clone()
    } else {
        return Ok(1);
    };

    let process_id = caller.data_mut().process_resources_mut().add(process);

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
) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap("lunatic::registry::get")?;
    let name = std::str::from_utf8(name).or_trap("lunatic::registry::get")?;

    state.registry().remove(name);

    Ok(())
}
