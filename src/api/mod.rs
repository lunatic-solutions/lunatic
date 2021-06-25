pub mod error;
mod plugin;
mod wasi;
pub(crate) mod process;

use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, IntoFunc, Linker, Memory, Trap, WasmRet, WasmTy};

use self::error::IntoTrap;
use crate::state::State;

// Registers all sub-APIs to the `Linker`
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &Vec<String>) -> Result<()> {
    plugin::register(linker, namespace_filter)?;
    process::register(linker, namespace_filter)?;
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
    namespace_filter: &Vec<String>,
) -> Result<()> {
    if namespace_matches_filter(namespace, name, namespace_filter) {
        linker.func_wrap(namespace, name, func)?;
    }
    Ok(())
}

// Adds async function to linker if the namespace matches the allowed list.
pub(crate) fn link_async4_if_match<T, A1, A2, A3, A4, R>(
    linker: &mut Linker<T>,
    namespace: &str,
    name: &str,
    func: impl for<'a> Fn(Caller<'a, T>, A1, A2, A3, A4) -> Box<dyn Future<Output = R> + Send + 'a>
        + Send
        + Sync
        + 'static,
    namespace_filter: &Vec<String>,
) -> Result<()>
where
    A1: WasmTy,
    A2: WasmTy,
    A3: WasmTy,
    A4: WasmTy,
    R: WasmRet,
{
    if namespace_matches_filter(namespace, name, namespace_filter) {
        linker.func_wrap4_async(namespace, name, func)?;
    }
    Ok(())
}

fn namespace_matches_filter(namespace: &str, name: &str, namespace_filter: &Vec<String>) -> bool {
    let full_name = format!("{}::{}", namespace, name);
    // Allow if any of the allowed namespaces matches the beginning of the full name.
    namespace_filter
        .iter()
        .any(|allowed| full_name.starts_with(allowed))
}
