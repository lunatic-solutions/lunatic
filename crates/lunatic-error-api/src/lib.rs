use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, link_if_match, IntoTrap};
use wasmtime::{Caller, Linker, ValType};
use wasmtime::{FuncType, Trap};

pub type ErrorResource = HashMapId<anyhow::Error>;

pub trait ErrorCtx {
    fn error_resources(&self) -> &ErrorResource;
    fn error_resources_mut(&mut self) -> &mut ErrorResource;
}

// Register the error APIs to the linker
pub fn register<T: ErrorCtx + 'static>(
    linker: &mut Linker<T>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::error",
        "string_size",
        FuncType::new([ValType::I64], [ValType::I32]),
        string_size::<T>,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::error",
        "to_string",
        FuncType::new([ValType::I64, ValType::I32], []),
        to_string::<T>,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::error",
        "drop",
        FuncType::new([ValType::I64], []),
        drop::<T>,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::error::string_size(error: u64) -> u32
//%
//% Returns the size of the string representation of the error.
//%
//% Traps:
//% * If the error ID doesn't exist.
fn string_size<T: ErrorCtx>(caller: Caller<T>, error_id: u64) -> Result<u32, Trap> {
    let error = caller
        .data()
        .error_resources()
        .get(error_id)
        .or_trap("lunatic::error::string_size")?;
    Ok(error.to_string().len() as u32)
}

//% lunatic::error::to_string(error_id: u64, error_str_ptr: u32)
//%
//% Write the string representation of the error to the guest memory.
//% `lunatic::error::string_size` can be called to get the string size.
//%
//% Traps:
//% * If the error ID doesn't exist.
//% * If **error_str_ptr + length of the error string** is outside the memory.
fn to_string<T: ErrorCtx>(
    mut caller: Caller<T>,
    error_id: u64,
    error_str_ptr: u32,
) -> Result<(), Trap> {
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

//% lunatic::error::drop(error_id: u64)
//%
//% Drops the error resource.
//%
//% Traps:
//% * If the error ID doesn't exist.
fn drop<T: ErrorCtx>(mut caller: Caller<T>, error_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .error_resources_mut()
        .remove(error_id)
        .or_trap("lunatic::error::drop")?;
    Ok(())
}
