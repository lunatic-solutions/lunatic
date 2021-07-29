use std::fmt::Display;

use anyhow::Result;
use wasmtime::{Caller, Linker, ValType};
use wasmtime::{FuncType, Trap};

use crate::{api::get_memory, state::ProcessState};

use super::link_if_match;

// Register the error APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::error",
        "string_size",
        FuncType::new([ValType::I64], [ValType::I32]),
        string_size,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::error",
        "to_string",
        FuncType::new([ValType::I64, ValType::I32], []),
        to_string,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::error",
        "drop",
        FuncType::new([ValType::I64], []),
        drop,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::error::string_size(error: u64) -> i32
//%
//% Returns the size of the string representation of the error.
//%
//% Traps:
//% * If the error ID doesn't exist.
fn string_size(caller: Caller<ProcessState>, error_id: u64) -> Result<u32, Trap> {
    let error = caller
        .data()
        .errors
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
fn to_string(
    mut caller: Caller<ProcessState>,
    error_id: u64,
    error_str_ptr: u32,
) -> Result<(), Trap> {
    let error = caller
        .data()
        .errors
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
fn drop(mut caller: Caller<ProcessState>, error_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .errors
        .remove(error_id)
        .or_trap("lunatic::error::drop")?;
    Ok(())
}

pub trait IntoTrap<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap>;
}

impl<T, E: Display> IntoTrap<T> for Result<T, E> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap> {
        match self {
            Ok(result) => Ok(result),
            Err(error) => Err(Trap::new(format!(
                "Trap raised during host call: {} ({}).",
                error, info
            ))),
        }
    }
}

impl<T> IntoTrap<T> for Option<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap> {
        match self {
            Some(result) => Ok(result),
            None => Err(Trap::new(format!(
                "Trap raised during host call: Expected `Some({})` got `None` ({}).",
                std::any::type_name::<T>(),
                info
            ))),
        }
    }
}
