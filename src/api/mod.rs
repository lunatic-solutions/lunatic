mod error;
#[macro_use]
mod macros;
mod mailbox;
mod networking;
mod plugin;
mod process;
mod wasi;

use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, IntoFunc, Linker, Memory, Trap, WasmRet, WasmTy};

use self::error::IntoTrap;
use crate::state::State;

// Registers all sub-APIs to the `Linker`
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &[String]) -> Result<()> {
    error::register(linker, namespace_filter)?;
    plugin::register(linker, namespace_filter)?;
    process::register(linker, namespace_filter)?;
    mailbox::register(linker, namespace_filter)?;
    networking::register(linker, namespace_filter)?;
    wasi::register(linker, namespace_filter)?;
    Ok(())
}

// Get exported memory
pub(crate) fn get_memory<T>(caller: &mut Caller<T>) -> std::result::Result<Memory, Trap> {
    caller
        .get_export("memory")
        .or_trap("No export `memory` found")?
        .into_memory()
        .or_trap("Export `memory` is not a memory")
}

// Adds function to linker if the namespace matches the allowed list.
pub(crate) fn link_if_match<T, Params, Results>(
    linker: &mut Linker<T>,
    namespace: &str,
    name: &str,
    func: impl IntoFunc<T, Params, Results>,
    namespace_filter: &[String],
) -> Result<()> {
    if namespace_matches_filter(namespace, name, namespace_filter) {
        linker.func_wrap(namespace, name, func)?;
    }
    Ok(())
}

// Adds link_async1_if_match, link_async2_if_match, ...
for_each_function_signature!(generate_wrap_async_func);

fn namespace_matches_filter(namespace: &str, name: &str, namespace_filter: &[String]) -> bool {
    let full_name = format!("{}::{}", namespace, name);
    // Allow if any of the allowed namespaces matches the beginning of the full name.
    namespace_filter
        .iter()
        .any(|allowed| full_name.starts_with(allowed))
}
