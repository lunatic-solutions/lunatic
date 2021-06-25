use std::future::Future;

use anyhow::Result;
use wasmtime::{Caller, Linker, Module, Trap};

use super::{get_memory, link_async4_if_match, link_if_match};
use crate::{
    api::error::IntoTrap,
    process::environment::{EnvConfig, Environment},
    state::{HashMapId, State},
};

// Register the process APIs to the linker
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &Vec<String>) -> Result<()> {
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
        "add_plugin",
        add_plugin,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "remove_last_plugin",
        remove_last_plugin,
        namespace_filter,
    )?;
    link_async4_if_match(
        linker,
        "lunatic::process",
        "crate_module",
        crate_module,
        namespace_filter,
    )?;

    Ok(())
}

#[derive(Default, Debug)]
pub(crate) struct PorcessState {
    pub(crate) configs: HashMapId<EnvConfig>,
    pub(crate) environments: HashMapId<Environment>,
    pub(crate) modules: HashMapId<Module>,
}

//% lunatic::process::create_config(max_memory: i64, max_fuel: i32) -> i64
//%
//% * **max_memory** - Maximum amount of memory in bytes that processes and plugins can use.
//% * **max_fuel**   - Maximum amount of instructions in gallons that processes will be able to run
//%                    before it traps. 1 gallon ~= 10k instructions. The special value of `0` means
//%                    unlimited instructions.
//% * Returns ID of newly created configuration.
//%
//% Create a new configuration for an environment.
fn create_config(mut caller: Caller<State>, max_memory: u64, max_fuel: u32) -> u64 {
    let max_fuel = if max_fuel != 0 { Some(max_fuel) } else { None };
    let config = EnvConfig::new(max_memory, max_fuel);
    caller.data_mut().process.configs.add(config)
}

//% lunatic::process::allow_namespace(config_id: i64, namespace_str_ptr: i32 namespace_str_len: i32)
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
        .process
        .configs
        .get_mut(config_id)
        .or_trap("lunatic::process::allow_namespace")?;
    config.allow_namespace(namespace);
    Ok(())
}

//% lunatic::process::create_environment(config_id: i64, i64_id_ptr: i64) -> i32
//%
//% Returns:
//% * 0 on success - The ID of the newly created environment is written to **i64_id_ptr**
//% * 1 on error   - The error ID is written to **i64_id_ptr**
//%
//% Consumes the config and creates a new environment from it.
//%
//% Traps:
//% * If the config ID doesn't exist.
//% * If **i64_id_ptr** is outside the memory.
fn create_environment(mut caller: Caller<State>, config_id: u64, id_ptr: u32) -> Result<i32, Trap> {
    let config = caller
        .data_mut()
        .process
        .configs
        .remove(config_id)
        .or_trap("lunatic::process::create_environment")?;

    let (env_or_error_id, result) = match Environment::new(config) {
        Ok(env) => (caller.data_mut().process.environments.add(env), 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &env_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::create_environment")?;

    Ok(result)
}

//% lunatic::process::add_plugin(
//%     env_id: i64,
//%     namespace_str_ptr: i32,
//%     namespace_str_len: i32,
//%     plugin_data_ptr: i32,
//%     plugin_data_len: i32,
//%     i64_id_ptr: i64
//% ) -> i32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - The error ID is written to **i64_id_ptr**
//%
//% Add plugin to environment. The order of adding plugins is significant because later plugins
//% can override functionality of earlier ones.
//%
//% Traps:
//% * If the environment ID doesn't exist.
//% * If **i64_id_ptr** is outside the memory.
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
        .process
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

//% lunatic::process::remove_last_plugin(env_id: i64)
//%
//% Removes last plugin from environment if there are plugins.
//%
//% Traps:
//% * If the environment ID doesn't exist.
fn remove_last_plugin(mut caller: Caller<State>, env_id: u64) -> Result<(), Trap> {
    let env = caller
        .data_mut()
        .process
        .environments
        .get_mut(env_id)
        .or_trap("lunatic::process::add_plugin")?;
    env.remove_last_plugin();
    Ok(())
}

//% lunatic::process::crate_module(
//%     env_id: i64,
//%     module_data_ptr: i32,
//%     module_data_len: i32,
//%     i64_id_ptr: i64
//% ) -> i64
//%
//% Returns:
//% * 0 on success - The ID of the newly created module is written to **i64_id_ptr**
//% * 1 on error   - The error ID is written to **i64_id_ptr**
//%
//% Creates a module from na environment. This function will also JIT compile the module.
//%
//% Traps:
//% * If the env ID doesn't exist.
//% * If **module_data_ptr + module_data_len** is outside the memory.
//% * If **i64_id_ptr** is outside the memory.
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
            .or_trap("lunatic::process::add_plugin")?;
        let env = caller
            .data_mut()
            .process
            .environments
            .get_mut(env_id)
            .or_trap("lunatic::process::crate_module")?;
        let (mod_or_error_id, result) = match env.create_module(module).await {
            Ok(module) => (caller.data_mut().process.modules.add(module), 0),
            Err(error) => (caller.data_mut().errors.add(error), 1),
        };
        memory
            .write(&mut caller, id_ptr as usize, &mod_or_error_id.to_le_bytes())
            .or_trap("lunatic::process::add_plugin")?;
        Ok(result)
    })
}

// spawn_by_name(module, function)
// spawn_by_index()
