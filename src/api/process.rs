use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, Linker, Trap};

use super::{get_memory, link_async4_if_match, link_async5_if_match, link_if_match};
use crate::{api::error::IntoTrap, state::State, EnvConfig, Environment};

// Register the process APIs to the linker
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &[String]) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::process",
        "create_config",
        create_config,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "drop_config",
        drop_config,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "allow_namespace",
        allow_namespace,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "create_environment",
        create_environment,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "drop_environment",
        drop_environment,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "add_plugin",
        add_plugin,
        namespace_filter,
    )?;
    link_async4_if_match(
        linker,
        "lunatic::process",
        "crate_module",
        crate_module,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "drop_module",
        drop_module,
        namespace_filter,
    )?;
    link_async5_if_match(linker, "lunatic::process", "spawn", spawn, namespace_filter)?;
    link_if_match(
        linker,
        "lunatic::process",
        "drop_process",
        drop_process,
        namespace_filter,
    )?;

    Ok(())
}

//% lunatic::process::create_config(max_memory: i64, max_fuel: i64) -> i64
//%
//% * **max_memory** - Maximum amount of memory in bytes that processes and plugins can use.
//% * **max_fuel**   - Maximum amount of instructions in gallons that processes will be able to run
//%                    before it traps. 1 gallon ~= 10k instructions. The special value of `0` means
//%                    unlimited instructions.
//% * Returns ID of newly created configuration.
//%
//% Create a new configuration for an environment.
fn create_config(mut caller: Caller<State>, max_memory: u64, max_fuel: u64) -> u64 {
    let max_fuel = if max_fuel != 0 { Some(max_fuel) } else { None };
    let config = EnvConfig::new(max_memory, max_fuel);
    caller.data_mut().resources.configs.add(config)
}

//% lunatic::error::drop_config(config_id: i64)
//%
//% Drops the config resource.
//%
//% Traps:
//% * If the config ID doesn't exist.
fn drop_config(mut caller: Caller<State>, config_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .configs
        .remove(config_id)
        .or_trap("lunatic::process::drop_config")?;
    Ok(())
}

//% lunatic::process::allow_namespace(config_id: i64, namespace_str_ptr: i32, namespace_str_len: i32)
//%
//% Allow using host functions under this namespace with this configuration. Namespaces are strings,
//% e.g. `lunatic::` or `lunatic::process::`.
//%
//% Traps:
//% * If the namespace string is not a valid utf8 string.
//% * If **namespace_str_ptr + namespace_str_len** is outside the memory.
fn allow_namespace(
    mut caller: Caller<State>,
    config_id: u64,
    namespace_str_ptr: u32,
    namespace_str_len: u32,
) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;
    let mut buffer = vec![0; namespace_str_len as usize];
    memory
        .read(&caller, namespace_str_ptr as usize, &mut buffer)
        .or_trap("lunatic::process::allow_namespace")?;
    let namespace =
        std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::process::allow_namespace")?;
    let config = caller
        .data_mut()
        .resources
        .configs
        .get_mut(config_id)
        .or_trap("lunatic::process::allow_namespace")?;
    config.allow_namespace(namespace);
    Ok(())
}

//% lunatic::process::create_environment(config_id: i64, id_ptr: i64) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created environment is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Consumes the config and creates a new environment from it.
//%
//% Traps:
//% * If the config ID doesn't exist.
//% * If **id_ptr** is outside the memory.
fn create_environment(mut caller: Caller<State>, config_id: u64, id_ptr: u32) -> Result<i32, Trap> {
    let config = caller
        .data_mut()
        .resources
        .configs
        .remove(config_id)
        .or_trap("lunatic::process::create_environment")?;

    let (env_or_error_id, result) = match Environment::new(config) {
        Ok(env) => (caller.data_mut().resources.environments.add(env), 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &env_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::create_environment")?;

    Ok(result)
}

//% lunatic::error::drop_environment(env_id: i64)
//%
//% Drops the environment resource.
//%
//% Traps:
//% * If the environment ID doesn't exist.
fn drop_environment(mut caller: Caller<State>, env_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .environments
        .remove(env_id)
        .or_trap("lunatic::process::drop_environment")?;
    Ok(())
}

//% lunatic::process::add_plugin(
//%     env_id: i64,
//%     namespace_str_ptr: i32,
//%     namespace_str_len: i32,
//%     plugin_data_ptr: i32,
//%     plugin_data_len: i32,
//%     id_ptr: i32
//% ) -> i32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Add plugin to environment. The order of adding plugins is significant because later plugins
//% can override functionality of earlier ones.
//%
//% Traps:
//% * If the environment ID doesn't exist.
//% * If **id_ptr** is outside the memory.
//% * If the namespace string is not a valid utf8 string.
//% * If **namespace_str_ptr + namespace_str_len** is outside the memory.
//% * If **plugin_data_ptr + plugin_data_len** is outside the memory.
fn add_plugin(
    mut caller: Caller<State>,
    env_id: u64,
    namespace_str_ptr: u32,
    namespace_str_len: u32,
    plugin_data_ptr: u32,
    plugin_data_len: u32,
    id_ptr: u32,
) -> Result<i32, Trap> {
    let mut plugin = vec![0; plugin_data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, plugin_data_ptr as usize, plugin.as_mut_slice())
        .or_trap("lunatic::process::add_plugin")?;

    let mut buffer = vec![0; namespace_str_len as usize];
    memory
        .read(&caller, namespace_str_ptr as usize, &mut buffer)
        .or_trap("lunatic::process::add_plugin")?;
    let namespace =
        std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::process::add_plugin")?;

    let env = caller
        .data_mut()
        .resources
        .environments
        .get_mut(env_id)
        .or_trap("lunatic::process::add_plugin")?;

    let (env_or_error_id, result) = match env.add_plugin(namespace, plugin) {
        Ok(()) => (0, 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &env_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::add_plugin")?;

    Ok(result)
}

//% lunatic::process::crate_module(
//%     env_id: i64,
//%     module_data_ptr: i32,
//%     module_data_len: i32,
//%     id_ptr: i32
//% ) -> i64
//%
//% Returns:
//% * 0 on success - The ID of the newly created module is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Creates a module from na environment. This function will also JIT compile the module.
//%
//% Traps:
//% * If the env ID doesn't exist.
//% * If **module_data_ptr + module_data_len** is outside the memory.
//% * If **id_ptr** is outside the memory.
fn crate_module(
    mut caller: Caller<State>,
    env_id: u64,
    module_data_ptr: u32,
    module_data_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<i32, Trap>> + Send + '_> {
    Box::new(async move {
        let mut module = vec![0; module_data_len as usize];
        let memory = get_memory(&mut caller)?;
        memory
            .read(&caller, module_data_ptr as usize, module.as_mut_slice())
            .or_trap("lunatic::process::crate_module")?;
        let env = caller
            .data_mut()
            .resources
            .environments
            .get_mut(env_id)
            .or_trap("lunatic::process::crate_module")?;
        let (mod_or_error_id, result) = match env.create_module(module).await {
            Ok(module) => (caller.data_mut().resources.modules.add(module), 0),
            Err(error) => (caller.data_mut().errors.add(error), 1),
        };
        memory
            .write(&mut caller, id_ptr as usize, &mod_or_error_id.to_le_bytes())
            .or_trap("lunatic::process::crate_module")?;
        Ok(result)
    })
}

//% lunatic::error::drop_module(mod_id: i64)
//%
//% Drops the module resource.
//%
//% Traps:
//% * If the module ID doesn't exist.
fn drop_module(mut caller: Caller<State>, mod_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .modules
        .remove(mod_id)
        .or_trap("lunatic::process::drop_module")?;
    Ok(())
}

//% lunatic::process::spawn(
//%     env_id: i64,
//%     env_id: u64,
//%     function_str_ptr: i32,
//%     function_str_len: i32,
//%     id_ptr: i32
//% ) -> i64
//%
//% Returns:
//% * 0 on success - The ID of the newly created process is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Spawns a new process using the passed in function inside a module as the entry point.
//%
//% Traps:
//% * If the env or module ID doesn't exist.
//% * If the function string is not a valid utf8 string.
//% * If **function_str_ptr + function_str_len** is outside the memory.
//% * If **id_ptr** is outside the memory.
fn spawn(
    mut caller: Caller<State>,
    env_id: u64,
    module_id: u64,
    function_str_ptr: u32,
    function_str_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<i32, Trap>> + Send + '_> {
    Box::new(async move {
        let mut buffer = vec![0; function_str_len as usize];
        let memory = get_memory(&mut caller)?;
        memory
            .read(&caller, function_str_ptr as usize, buffer.as_mut_slice())
            .or_trap("lunatic::process::spawn")?;
        let function = std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::process::spawn")?;
        let env = caller
            .data()
            .resources
            .environments
            .get(env_id)
            .or_trap("lunatic::process::spawn")?;
        let module = caller
            .data()
            .resources
            .modules
            .get(module_id)
            .or_trap("lunatic::process::spawn")?;
        let (mod_or_error_id, result) = match env.spawn(module, &function).await {
            Ok(process) => (caller.data_mut().resources.processes.add(process), 0),
            Err(error) => (caller.data_mut().errors.add(error), 1),
        };
        memory
            .write(&mut caller, id_ptr as usize, &mod_or_error_id.to_le_bytes())
            .or_trap("lunatic::process::spawn")?;
        Ok(result)
    })
}

//% lunatic::process::drop_process(process_id: i64)
//%
//% Drops the process handle. This will not kill the process, it just removes the handle that
//% references the process and allows us to send messages and signals to it.
//%
//% Traps:
//% * If the process ID doesn't exist.
fn drop_process(mut caller: Caller<State>, process_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .processes
        .remove(process_id)
        .or_trap("lunatic::process::drop_process")?;
    Ok(())
}
