use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, IntoTrap};
use wasmtime::{Caller, Linker};

pub type ErrorResource = HashMapId<anyhow::Error>;

pub trait ErrorCtx {
    fn error_resources(&self) -> &ErrorResource;
    fn error_resources_mut(&mut self) -> &mut ErrorResource;
}

// Register the error APIs to the linker
pub fn register<T: ErrorCtx + 'static>(linker: &mut Linker<T>) -> Result<()> {
    linker.func_wrap("lunatic::error", "string_size", string_size)?;
    linker.func_wrap("lunatic::error", "to_string", to_string)?;
    linker.func_wrap("lunatic::error", "drop", drop)?;
    Ok(())
}

// Returns the size of the string representation of the error.
//
// Traps:
// * If the error ID doesn't exist.
fn string_size<T: ErrorCtx>(caller: Caller<T>, error_id: u64) -> Result<u32> {
    let error = caller
        .data()
        .error_resources()
        .get(error_id)
        .or_trap("lunatic::error::string_size")?;
    Ok(error.to_string().len() as u32)
}

// Writes the string representation of the error to the guest memory.
// `lunatic::error::string_size` can be used to get the string size.
//
// Traps:
// * If the error ID doesn't exist.
// * If any memory outside the guest heap space is referenced.
fn to_string<T: ErrorCtx>(mut caller: Caller<T>, error_id: u64, error_str_ptr: u32) -> Result<()> {
    let error = caller
        .data()
        .error_resources()
        .get(error_id)
        .or_trap("lunatic::error::string_size")?;
    let error_str = error.to_string();
    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, error_str_ptr as usize, error_str.as_ref())
        .or_trap("lunatic::error::string_size")?;
    Ok(())
}

// Drops the error resource.
//
// Traps:
// * If the error ID doesn't exist.
fn drop<T: ErrorCtx>(mut caller: Caller<T>, error_id: u64) -> Result<()> {
    caller
        .data_mut()
        .error_resources_mut()
        .remove(error_id)
        .or_trap("lunatic::error::drop")?;
    Ok(())
}
