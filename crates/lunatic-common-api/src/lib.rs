#[macro_use]
mod macros;

use anyhow::Result;
use std::{fmt::Display, future::Future};
use wasmtime::{Caller, FuncType, IntoFunc, Linker, Memory, Trap, WasmRet, WasmTy};

// Get exported memory
pub fn get_memory<T>(caller: &mut Caller<T>) -> std::result::Result<Memory, Trap> {
    caller
        .get_export("memory")
        .or_trap("No export `memory` found")?
        .into_memory()
        .or_trap("Export `memory` is not a memory")
}

// Adds function to linker if the namespace matches the allowed list.
pub fn link_if_match<T, Params, Results>(
    linker: &mut Linker<T>,
    namespace: &str,
    name: &str,
    func_ty: FuncType,
    func: impl IntoFunc<T, Params, Results>,
    namespace_filter: &[String],
) -> Result<()> {
    if namespace_matches_filter(namespace, name, namespace_filter) {
        linker.func_wrap(namespace, name, func)?;
    } else {
        // If the host function is forbidden, we still want to add a fake function that always
        // traps under its name. This allows us to spawn a module into different environments,
        // even not all parts of the module can be run inside an environment.
        let error = format!(
            "Host function `{}::{}` unavailable in this environment.",
            namespace, name
        );
        linker.func_new(namespace, name, func_ty, move |_, _, _| {
            Err(Trap::new(error.clone()))
        })?;
    }
    Ok(())
}

// Adds link_async1_if_match, link_async2_if_match, ...
for_each_function_signature!(generate_wrap_async_func);

pub fn namespace_matches_filter(namespace: &str, name: &str, namespace_filter: &[String]) -> bool {
    let full_name = format!("{}::{}", namespace, name);
    // Allow if any of the allowed namespaces matches the beginning of the full name.
    namespace_filter
        .iter()
        .any(|allowed| full_name.starts_with(allowed))
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
