mod error;
#[macro_use]
mod macros;
mod mailbox;
mod networking;
pub(crate) mod plugin;
mod process;
mod wasi;

use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, FuncType, IntoFunc, Linker, Memory, Trap, WasmRet, WasmTy};

use self::error::IntoTrap;
use crate::state::ProcessState;

// Registers all sub-APIs to the `Linker`
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    error::register(linker, namespace_filter)?;
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

fn namespace_matches_filter(namespace: &str, name: &str, namespace_filter: &[String]) -> bool {
    let full_name = format!("{}::{}", namespace, name);
    // Allow if any of the allowed namespaces matches the beginning of the full name.
    namespace_filter
        .iter()
        .any(|allowed| full_name.starts_with(allowed))
}

mod tests {
    #[tokio::test]
    async fn import_filter_signature_matches() {
        use crate::{EnvConfig, Environment};

        // The default configuration includes both, the "lunatic::*" and "wasi_*" namespaces.
        let config = EnvConfig::default();
        let environment = Environment::new(config).unwrap();
        let raw_module = std::fs::read("./target/wasm/all_imports.wasm").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();

        // This configuration should still compile, even all host calls will trap.
        let config = EnvConfig::new(0, None);
        let environment = Environment::new(config).unwrap();
        let raw_module = std::fs::read("./target/wasm/all_imports.wasm").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();
    }
}
