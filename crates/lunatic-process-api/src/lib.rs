use std::{
    convert::{TryFrom, TryInto},
    future::Future,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Result};
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;
use lunatic_process::{
    config::ProcessConfig, mailbox::MessageMailbox, message::Message,
    runtimes::wasmtime::WasmtimeCompiledModule, state::ProcessState, wasm::spawn_wasm, Process,
    Signal, WasmProcess,
};
use wasmtime::{Caller, Linker, ResourceLimiter, Trap, Val};

pub type ConfigResources<T> = HashMapId<T>;
pub type ProcessResources = HashMapId<Arc<dyn Process>>;
pub type ModuleResources<T> = HashMapId<WasmtimeCompiledModule<T>>;

pub trait ProcessCtx<S: ProcessState> {
    fn mailbox(&mut self) -> &mut MessageMailbox;
    fn message_scratch_area(&mut self) -> &mut Option<Message>;
    fn module_resources(&self) -> &ModuleResources<S>;
    fn module_resources_mut(&mut self) -> &mut ModuleResources<S>;
    fn config_resources(&self) -> &ConfigResources<S::Config>;
    fn config_resources_mut(&mut self) -> &mut ConfigResources<S::Config>;
    fn process_resources(&self) -> &ProcessResources;
    fn process_resources_mut(&mut self) -> &mut ProcessResources;
}

// Register the process APIs to the linker
pub fn register<T>(linker: &mut Linker<T>) -> Result<()>
where
    T: ProcessState + ProcessCtx<T> + ErrorCtx + Send + ResourceLimiter + 'static,
    for<'a> &'a T: Send,
{
    linker.func_wrap("lunatic::process", "create_config", create_config)?;
    linker.func_wrap("lunatic::process", "drop_config", drop_config)?;
    linker.func_wrap(
        "lunatic::process",
        "config_set_max_memory",
        config_set_max_memory,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_get_max_memory",
        config_get_max_memory,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_set_max_fuel",
        config_set_max_fuel,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_get_max_fuel",
        config_get_max_fuel,
    )?;
    linker.func_wrap8_async("lunatic::process", "spawn", spawn)?;

    linker.func_wrap("lunatic::process", "drop_process", drop_process)?;
    linker.func_wrap("lunatic::process", "clone_process", clone_process)?;
    linker.func_wrap1_async("lunatic::process", "sleep_ms", sleep_ms)?;
    linker.func_wrap("lunatic::process", "die_when_link_dies", die_when_link_dies)?;
    linker.func_wrap("lunatic::process", "this", this)?;

    linker.func_wrap("lunatic::process", "id", id)?;
    linker.func_wrap("lunatic::process", "link", link)?;
    linker.func_wrap("lunatic::process", "unlink", unlink)?;

    Ok(())
}

// Create a new configuration with all permissions denied.
//
// There is no memory or fuel limit set on the newly created configuration.
//
// Returns:
// * ID of newly created configuration.
fn create_config<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>) -> u64 {
    let config = T::Config::default();
    caller.data_mut().config_resources_mut().add(config)
}

// Drops the configuration from resources.
//
// Traps:
// * If the config ID doesn't exist.
fn drop_config<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    config_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .config_resources_mut()
        .remove(config_id)
        .or_trap("lunatic::process::drop_config: Config ID doesn't exist")?;
    Ok(())
}

// Sets the memory limit on a configuration.
//
// Traps:
// * If max_memory is bigger than the platform maximum.
// * If the config ID doesn't exist.
fn config_set_max_memory<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    config_id: u64,
    max_memory: u64,
) -> Result<(), Trap> {
    let max_memory = usize::try_from(max_memory)
        .or_trap("lunatic::process::config_set_max_memory: max_memory exceeds platform max")?;
    caller
        .data_mut()
        .config_resources_mut()
        .get_mut(config_id)
        .or_trap("lunatic::process::config_set_max_memory: Config ID doesn't exist")?
        .set_max_memory(max_memory);
    Ok(())
}

// Returns the memory limit of a configuration.
//
// Traps:
// * If the config ID doesn't exist.
fn config_get_max_memory<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    config_id: u64,
) -> Result<u64, Trap> {
    let max_memory = caller
        .data_mut()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_get_max_memory: Config ID doesn't exist")?
        .get_max_memory();
    Ok(max_memory as u64)
}

// Sets the fuel limit on a configuration.
//
// A value of 0 indicates no fuel limit.
//
// Traps:
// * If the config ID doesn't exist.
fn config_set_max_fuel<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    config_id: u64,
    max_fuel: u64,
) -> Result<(), Trap> {
    let max_fuel = match max_fuel {
        0 => None,
        max_fuel => Some(max_fuel),
    };

    caller
        .data_mut()
        .config_resources_mut()
        .get_mut(config_id)
        .or_trap("lunatic::process::config_set_max_fuel: Config ID doesn't exist")?
        .set_max_fuel(max_fuel);
    Ok(())
}

// Returns the fuel limit of a configuration.
//
// A value of 0 indicates no fuel limit.
//
// Traps:
// * If the config ID doesn't exist.
fn config_get_max_fuel<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    config_id: u64,
) -> Result<u64, Trap> {
    let max_fuel = caller
        .data_mut()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_get_max_fuel: Config ID doesn't exist")?
        .get_max_fuel();
    match max_fuel {
        None => Ok(0),
        Some(max_fuel) => Ok(max_fuel),
    }
}

// Spawns a new process using the passed in function inside a module as the entry point.
//
// If **link** is not 0, it will link the child and parent processes. The value of the **link**
// argument will be used as the link-tag for the child. This means, if the child traps the parent
// is going to get a signal back with the value used as the tag.
//
// The function arguments are passed as an array with the following structure:
// [0 byte = type ID; 1..17 bytes = value as u128, ...]
// The type ID follows the WebAssembly binary convention:
//  - 0x7F => i32
//  - 0x7E => i64
//  - 0x7B => v128
// If any other value is used as type ID, this function will trap.
//
// Returns:
// * 0 on success - The ID of the newly created process is written to **id_ptr**
// * 1 on error   - The error ID is written to **id_ptr**
//
// Traps:
// * If the module index doesn't exist.
// * If the function string is not a valid utf8 string.
// * If the params array is in a wrong format.
// * If **func_str_ptr + func_str_len** is outside the memory.
// * If **params_ptr + params_len** is outside the memory.
// * If **id_ptr** is outside the memory.
#[allow(clippy::too_many_arguments)]
fn spawn<T>(
    mut caller: Caller<T>,
    link: i64,
    config_id: i64,
    module_id: i64,
    func_str_ptr: u32,
    func_str_len: u32,
    params_ptr: u32,
    params_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32, Trap>> + Send + '_>
where
    T: ProcessState + ProcessCtx<T> + ErrorCtx + ResourceLimiter + Send + 'static,
    for<'a> &'a T: Send,
{
    Box::new(async move {
        let state = caller.data();
        if !state.is_initialized() {
            return Err(anyhow!("Cannot spawn process during module initialization").into());
        }

        let config = match config_id {
            -1 => state.config().clone(),
            config_id => Arc::new(
                caller
                    .data()
                    .config_resources()
                    .get(config_id as u64)
                    .or_trap("lunatic::process::spawn: Config ID doesn't exist")?
                    .clone(),
            ),
        };

        let module = match module_id {
            -1 => state.module().clone(),
            module_id => caller
                .data()
                .module_resources()
                .get(module_id as u64)
                .or_trap("lunatic::process::spawn: Config ID doesn't exist")?
                .clone(),
        };

        let memory = get_memory(&mut caller)?;
        let func_str = memory
            .data(&caller)
            .get(func_str_ptr as usize..(func_str_ptr + func_str_len) as usize)
            .or_trap("lunatic::process::spawn")?;
        let function = std::str::from_utf8(func_str).or_trap("lunatic::process::spawn")?;
        let params = memory
            .data(&caller)
            .get(params_ptr as usize..(params_ptr + params_len) as usize)
            .or_trap("lunatic::process::spawn")?;
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
        let link: Option<(Option<i64>, Arc<dyn Process>)> = match link {
            0 => None,
            tag => {
                let id = caller.data().id();
                let signal_mailbox = caller.data().signal_mailbox().clone();
                let process = WasmProcess::new(id, signal_mailbox);
                Some((Some(tag), Arc::new(process)))
            }
        };
        let runtime = caller.data().runtime().clone();
        let (proc_or_error_id, result) =
            match spawn_wasm(runtime, module, config, function, params, link).await {
                Ok((_, process)) => (caller.data_mut().process_resources_mut().add(process), 0),
                Err(error) => (caller.data_mut().error_resources_mut().add(error), 1),
            };
        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &proc_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::process::spawn")?;
        Ok(result)
    })
}

// Drops the process handle. This will not kill the process, it just removes the handle that
// references the process and allows us to send messages and signals to it.
//
// Traps:
// * If the process ID doesn't exist.
fn drop_process<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    process_id: u64,
) -> Result<(), Trap> {
    caller
        .data_mut()
        .process_resources_mut()
        .remove(process_id)
        .or_trap("lunatic::process::drop_process")?;
    Ok(())
}

// Clones a process returning the ID of the clone.
//
// Traps:
// * If the process ID doesn't exist.
fn clone_process<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    process_id: u64,
) -> Result<u64, Trap> {
    let process = caller
        .data()
        .process_resources()
        .get(process_id)
        .or_trap("lunatic::process::clone_process")?
        .clone();
    let id = caller.data_mut().process_resources_mut().add(process);
    Ok(id)
}

// lunatic::process::sleep_ms(millis: u64)
//
// Suspend process for `millis`.
fn sleep_ms<T: ProcessState + ProcessCtx<T>>(
    _: Caller<T>,
    millis: u64,
) -> Box<dyn Future<Output = ()> + Send + '_> {
    Box::new(async move {
        async_std::task::sleep(Duration::from_millis(millis)).await;
    })
}

// Defines what happens to this process if one of the linked processes notifies us that it died.
//
// There are 2 options:
// 1. `trap == 0` the received signal will be turned into a signal message and put into the mailbox.
// 2. `trap != 0` the process will die and notify all linked processes of its death.
//
// The default behaviour for a newly spawned process is 2.
fn die_when_link_dies<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>, trap: u32) {
    caller
        .data_mut()
        .signal_mailbox()
        .try_send(Signal::DieWhenLinkDies(trap != 0))
        .expect("The signal is sent to itself and the receiver must exist at this point");
}

// Create a process handle to itself and return resource ID.
fn this<T: ProcessState + ProcessCtx<T>>(mut caller: Caller<T>) -> u64 {
    let id = caller.data().id();
    let signal_mailbox = caller.data().signal_mailbox().clone();
    let process = WasmProcess::new(id, signal_mailbox);
    caller
        .data_mut()
        .process_resources_mut()
        .add(Arc::new(process))
}

// Returns UUID of a process as u128_ptr.
//
// Traps:
// * If the process ID doesn't exist.
// * If **u128_ptr** is outside the memory space.
fn id<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    process_id: u64,
    u128_ptr: u32,
) -> Result<(), Trap> {
    let id = caller
        .data()
        .process_resources()
        .get(process_id)
        .or_trap("lunatic::process::id")?
        .id()
        .as_u128();
    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, u128_ptr as usize, &id.to_le_bytes())
        .or_trap("lunatic::process::id")?;
    Ok(())
}

// Link current process to **process_id**. This is not an atomic operation, any of the 2 processes
// could fail before processing the `Link` signal and may not notify the other.
//
// Traps:
// * If the process ID doesn't exist.
fn link<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    tag: i64,
    process_id: u64,
) -> Result<(), Trap> {
    let tag = match tag {
        0 => None,
        tag => Some(tag),
    };
    // Create handle to itself
    let id = caller.data().id();
    let signal_mailbox = caller.data().signal_mailbox().clone();
    let this_process = WasmProcess::new(id, signal_mailbox);

    // Send link signal to other process
    let process = caller
        .data()
        .process_resources()
        .get(process_id)
        .or_trap("lunatic::process::link")?
        .clone();
    process.send(Signal::Link(tag, Arc::new(this_process)));

    // Send link signal to itself
    caller
        .data_mut()
        .signal_mailbox()
        .try_send(Signal::Link(tag, process))
        .expect("The signal is sent to itself and the receiver must exist at this point");
    Ok(())
}

// Unlink current process from **process_id**. This is not an atomic operation.
//
// Traps:
// * If the process ID doesn't exist.
fn unlink<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    process_id: u64,
) -> Result<(), Trap> {
    // Create handle to itself
    let id = caller.data().id();
    let signal_mailbox = caller.data().signal_mailbox().clone();
    let this_process = WasmProcess::new(id, signal_mailbox);

    // Send unlink signal to other process
    let process = caller
        .data()
        .process_resources()
        .get(process_id)
        .or_trap("lunatic::process::link")?
        .clone();
    process.send(Signal::UnLink(Arc::new(this_process)));

    // Send unlink signal to itself
    caller
        .data_mut()
        .signal_mailbox()
        .try_send(Signal::UnLink(process))
        .expect("The signal is sent to itself and the receiver must exist at this point");
    Ok(())
}
