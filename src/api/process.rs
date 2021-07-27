use std::{convert::TryInto, future::Future, time::Duration};

use anyhow::{anyhow, Result};
use wasmtime::{Caller, Linker, Trap, Val};

use super::{
    get_memory, link_async1_if_match, link_async4_if_match, link_async6_if_match,
    link_async7_if_match, link_if_match,
};
use crate::{
    api::error::IntoTrap,
    module::Module,
    process::{ProcessHandle, Signal},
    state::ProcessState,
    EnvConfig, Environment,
};

// Register the process APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
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
    link_async7_if_match(linker, "lunatic::process", "spawn", spawn, namespace_filter)?;
    link_async6_if_match(
        linker,
        "lunatic::process",
        "inherit_spawn",
        inherit_spawn,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "drop_process",
        drop_process,
        namespace_filter,
    )?;
    link_async1_if_match(
        linker,
        "lunatic::process",
        "sleep_ms",
        sleep_ms,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::process",
        "die_when_link_dies",
        die_when_link_dies,
        namespace_filter,
    )?;
    link_if_match(linker, "lunatic::process", "this", this, namespace_filter)?;
    link_async1_if_match(linker, "lunatic::process", "join", join, namespace_filter)?;
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
fn create_config(mut caller: Caller<ProcessState>, max_memory: u64, max_fuel: u64) -> u64 {
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
fn drop_config(mut caller: Caller<ProcessState>, config_id: u64) -> Result<(), Trap> {
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
    mut caller: Caller<ProcessState>,
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

//% lunatic::process::add_plugin(
//%     config_id: i64,
//%     plugin_data_ptr: i32,
//%     plugin_data_len: i32,
//%     id_ptr: i32
//% ) -> i32
//%
//% Returns:
//% * 0 on success
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Add plugin to environment configuration.
//%
//% Traps:
//% * If the config ID doesn't exist.
//% * If **id_ptr** is outside the memory.
//% * If **plugin_data_ptr + plugin_data_len** is outside the memory.
fn add_plugin(
    mut caller: Caller<ProcessState>,
    config_id: u64,
    plugin_data_ptr: u32,
    plugin_data_len: u32,
    id_ptr: u32,
) -> Result<i32, Trap> {
    let mut plugin = vec![0; plugin_data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, plugin_data_ptr as usize, plugin.as_mut_slice())
        .or_trap("lunatic::process::add_plugin")?;

    let config = caller
        .data_mut()
        .resources
        .configs
        .get_mut(config_id)
        .or_trap("lunatic::process::add_plugin")?;

    let (env_or_error_id, result) = match config.add_plugin(plugin) {
        Ok(()) => (0, 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &env_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::add_plugin")?;

    Ok(result)
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
fn create_environment(
    mut caller: Caller<ProcessState>,
    config_id: u64,
    id_ptr: u32,
) -> Result<i32, Trap> {
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
fn drop_environment(mut caller: Caller<ProcessState>, env_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .environments
        .remove(env_id)
        .or_trap("lunatic::process::drop_environment")?;
    Ok(())
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
    mut caller: Caller<ProcessState>,
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
fn drop_module(mut caller: Caller<ProcessState>, mod_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .modules
        .remove(mod_id)
        .or_trap("lunatic::process::drop_module")?;
    Ok(())
}

//% lunatic::process::spawn(
//%     link: u32,
//%     module_id: u64,
//%     func_str_ptr: i32,
//%     func_str_len: i32,
//%     params_ptr: i32,
//%     params_len: i32,
//%     id_ptr: i32
//% ) -> i64
//%
//% Returns:
//% * 0 on success - The ID of the newly created process is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Spawns a new process using the passed in function inside a module as the entry point.
//% If link is not 0, it will link the child
//%
//% The function arguments are passed as an array with the following structure:
//% [0 byte = type ID; 1..17 bytes = value as u128, ...]
//% The type ID follows the WebAssembly binary convention:
//%  - 0x7F => i32
//%  - 0x7E => i64
//%  - 0x7B => v128
//% If any other value is used as type ID, this function will trap.
//%
//% Traps:
//% * If the module ID doesn't exist.
//% * If the function string is not a valid utf8 string.
//% * If the params array is in a wrong format.
//% * If **func_str_ptr + func_str_len** is outside the memory.
//% * If **params_ptr + params_len** is outside the memory.
//% * If **id_ptr** is outside the memory.
fn spawn(
    mut caller: Caller<ProcessState>,
    link: u32,
    module_id: u64,
    func_str_ptr: u32,
    func_str_len: u32,
    params_ptr: u32,
    params_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<i32, Trap>> + Send + '_> {
    Box::new(async move {
        let module = caller
            .data()
            .resources
            .modules
            .get(module_id)
            .or_trap("lunatic::process::spawn")?
            .clone();
        spawn_from_module(
            &mut caller,
            link,
            module,
            func_str_ptr,
            func_str_len,
            params_ptr,
            params_len,
            id_ptr,
        )
        .await
    })
}

//% lunatic::process::inherit_spawn(
//%     func_str_ptr: i32,
//%     func_str_len: i32,
//%     params_ptr: i32,
//%     params_len: i32,
//%     id_ptr: i32
//% ) -> i64
//%
//% Returns:
//% * 0 on success - The ID of the newly created process is written to **id_ptr**
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Spawns a new process using the same module as the parent.
//% If **link** is not 0, it will link the child and parent processes.
//%
//% The function arguments are passed as an array with the following structure:
//% [0 byte = type ID; 1..17 bytes = value as u128, ...]
//% The type ID follows the WebAssembly binary convention:
//%  - 0x7F => i32
//%  - 0x7E => i64
//%  - 0x7B => v128
//% If any other value is used as type ID, this function will trap.
//%
//% Traps:
//% * If the function string is not a valid utf8 string.
//% * If the params array is in a wrong format.
//% * If **func_str_ptr + func_str_len** is outside the memory.
//% * If **params_ptr + params_len** is outside the memory.
//% * If **id_ptr** is outside the memory.
fn inherit_spawn(
    mut caller: Caller<ProcessState>,
    link: u32,
    func_str_ptr: u32,
    func_str_len: u32,
    params_ptr: u32,
    params_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<i32, Trap>> + Send + '_> {
    Box::new(async move {
        let module = caller.data().module.clone();
        spawn_from_module(
            &mut caller,
            link,
            module,
            func_str_ptr,
            func_str_len,
            params_ptr,
            params_len,
            id_ptr,
        )
        .await
    })
}

async fn spawn_from_module(
    mut caller: &mut Caller<'_, ProcessState>,
    link: u32,
    module: Module,
    func_str_ptr: u32,
    func_str_len: u32,
    params_ptr: u32,
    params_len: u32,
    id_ptr: u32,
) -> Result<i32, Trap> {
    let memory = get_memory(&mut caller)?;
    let func_str = memory
        .data(&caller)
        .get(func_str_ptr as usize..(func_str_ptr + func_str_len) as usize)
        .or_trap("lunatic::process::(inherit_)spawn")?;
    let function = std::str::from_utf8(func_str).or_trap("lunatic::process::(inherit_)spawn")?;
    let params = memory
        .data(&caller)
        .get(params_ptr as usize..(params_ptr + params_len) as usize)
        .or_trap("lunatic::process::(inherit_)spawn")?;
    let params = params
        .chunks_exact(17)
        .map(|chunk| {
            let value = u128::from_le_bytes(chunk[1..].try_into()?);
            let result = match chunk[0] {
                0x7F => Val::I32(value as i32),
                0x7E => Val::I64(value as i64),
                0x7B => Val::V128(value),
                _ => return Err(anyhow!("Unsupported type ID")),
            };
            Ok(result)
        })
        .collect::<Result<Vec<_>>>()?;
    // Should processes be linked together?
    let link = match link {
        0 => None,
        _ => {
            let id = caller.data().id.clone();
            let trapped_sender = caller.data().trapped_sender.clone();
            let signal_sender = caller.data().signal_sender.clone();
            let message_sender = caller.data().message_sender.clone();
            let process = ProcessHandle::new(id, signal_sender, message_sender, trapped_sender);
            Some(process)
        }
    };
    let (mod_or_error_id, result) = match module.spawn(&function, params, link).await {
        Ok((_, process)) => (caller.data_mut().resources.processes.add(process), 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };
    memory
        .write(&mut caller, id_ptr as usize, &mod_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::(inherit_)spawn")?;
    Ok(result)
}

//% lunatic::process::drop_process(process_id: i64)
//%
//% Drops the process handle. This will not kill the process, it just removes the handle that
//% references the process and allows us to send messages and signals to it.
//%
//% Traps:
//% * If the process ID doesn't exist.
fn drop_process(mut caller: Caller<ProcessState>, process_id: u64) -> Result<(), Trap> {
    caller
        .data_mut()
        .resources
        .processes
        .remove(process_id)
        .or_trap("lunatic::process::drop_process")?;
    Ok(())
}

//% lunatic::process::sleep_ms(millis: i64)
//%
//% Suspend process for `millis`.
fn sleep_ms(_: Caller<ProcessState>, millis: u64) -> Box<dyn Future<Output = ()> + Send + '_> {
    Box::new(async move {
        tokio::time::sleep(Duration::from_millis(millis)).await;
    })
}

//% lunatic::error::die_when_link_dies(trap: u32)
//%
//% Defines what happens to this process if one of the linked processes notifies us that it died.
//%
//% There are 2 options:
//% 1. `trap == 0` the received signal will be turned into a signal message and put into the mailbox.
//% 2. `trap != 0` the process will die and notify all linked processes of its death.
//%
//% The default behaviour for a newly spawned process is 2.
fn die_when_link_dies(mut caller: Caller<ProcessState>, trap: u32) {
    caller
        .data_mut()
        .signal_sender
        .send(Signal::DieWhenLinkDies(trap != 0))
        .expect("The signal is sent to itself and the receiver must exist at this point");
}

//% lunatic::error::this() -> u64
//%
//% Create a process handle to itself and return resource ID.
fn this(mut caller: Caller<ProcessState>) -> u64 {
    let id = caller.data().id.clone();
    let trapped_sender = caller.data().trapped_sender.clone();
    let signal_sender = caller.data().signal_sender.clone();
    let message_sender = caller.data().message_sender.clone();
    let process = ProcessHandle::new(id, signal_sender, message_sender, trapped_sender);
    caller.data_mut().resources.processes.add(process)
}

//% lunatic::error::join(process_id: u64) -> u32
//%
//% Returns:
//% * 0 if process finished normally.
//% * 1 ir process trapped or received a Signal::Kill.
//%
//% Blocks until the process finishes and returns the status code.
//%
//% Traps:
//% * If the process ID doesn't exist.
fn join(
    mut caller: Caller<ProcessState>,
    process_id: u64,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_> {
    Box::new(async move {
        let process = caller
            .data_mut()
            .resources
            .processes
            .get_mut(process_id)
            .or_trap("lunatic::process::join")?;
        if process.join().await {
            Ok(1)
        } else {
            Ok(0)
        }
    })
}
