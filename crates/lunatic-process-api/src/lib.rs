use std::{
    convert::{TryFrom, TryInto},
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;
use lunatic_process::{
    config::ProcessConfig,
    env::Environment,
    mailbox::MessageMailbox,
    message::Message,
    runtimes::{wasmtime::WasmtimeCompiledModule, RawWasm},
    state::ProcessState,
    DeathReason, Process, Signal, WasmProcess,
};
use lunatic_wasi_api::LunaticWasiCtx;
use wasmtime::{Caller, Linker, ResourceLimiter, Trap, Val};

pub type ProcessResources = HashMapId<Arc<dyn Process>>;
pub type ModuleResources<S> = HashMapId<Arc<WasmtimeCompiledModule<S>>>;

pub trait ProcessConfigCtx {
    fn can_compile_modules(&self) -> bool;
    fn set_can_compile_modules(&mut self, can: bool);
    fn can_create_configs(&self) -> bool;
    fn set_can_create_configs(&mut self, can: bool);
    fn can_spawn_processes(&self) -> bool;
    fn set_can_spawn_processes(&mut self, can: bool);
}

pub trait ProcessCtx<S: ProcessState> {
    fn mailbox(&mut self) -> &mut MessageMailbox;
    fn message_scratch_area(&mut self) -> &mut Option<Message>;
    fn module_resources(&self) -> &ModuleResources<S>;
    fn module_resources_mut(&mut self) -> &mut ModuleResources<S>;
    fn environment(&self) -> Arc<dyn Environment>;
}

// Register the process APIs to the linker
pub fn register<T>(linker: &mut Linker<T>) -> Result<()>
where
    T: ProcessState + ProcessCtx<T> + ErrorCtx + LunaticWasiCtx + Send + ResourceLimiter + 'static,
    for<'a> &'a T: Send,
    T::Config: ProcessConfigCtx,
{
    #[cfg(feature = "metrics")]
    lunatic_process::describe_metrics();

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.process.modules.compiled",
        metrics::Unit::Count,
        "number of modules compiled since startup"
    );

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.process.modules.dropped",
        metrics::Unit::Count,
        "number of modules dropped since startup"
    );

    #[cfg(feature = "metrics")]
    metrics::describe_gauge!(
        "lunatic.process.modules.active",
        metrics::Unit::Count,
        "number of modules currently in memory"
    );

    #[cfg(feature = "metrics")]
    metrics::describe_histogram!(
        "lunatic.process.modules.compiled.duration",
        metrics::Unit::Seconds,
        "Duration of module compilation"
    );

    linker.func_wrap("lunatic::process", "compile_module", compile_module)?;
    linker.func_wrap("lunatic::process", "drop_module", drop_module)?;

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.process.configs.created",
        metrics::Unit::Count,
        "number of configs created since startup"
    );

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.process.configs.dropped",
        metrics::Unit::Count,
        "number of configs dropped since startup"
    );

    #[cfg(feature = "metrics")]
    metrics::describe_gauge!(
        "lunatic.process.configs.active",
        metrics::Unit::Count,
        "number of configs currently in memory"
    );

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
    linker.func_wrap(
        "lunatic::process",
        "config_can_compile_modules",
        config_can_compile_modules,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_set_can_compile_modules",
        config_set_can_compile_modules,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_can_create_configs",
        config_can_create_configs,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_set_can_create_configs",
        config_set_can_create_configs,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_can_spawn_processes",
        config_can_spawn_processes,
    )?;
    linker.func_wrap(
        "lunatic::process",
        "config_set_can_spawn_processes",
        config_set_can_spawn_processes,
    )?;

    linker.func_wrap8_async("lunatic::process", "spawn", spawn)?;

    linker.func_wrap1_async("lunatic::process", "sleep_ms", sleep_ms)?;
    linker.func_wrap("lunatic::process", "die_when_link_dies", die_when_link_dies)?;

    linker.func_wrap("lunatic::process", "process_id", process_id)?;
    linker.func_wrap("lunatic::process", "environment_id", environment_id)?;
    linker.func_wrap("lunatic::process", "link", link)?;
    linker.func_wrap("lunatic::process", "unlink", unlink)?;
    linker.func_wrap("lunatic::process", "kill", kill)?;
    linker.func_wrap("lunatic::process", "exists", exists)?;
    Ok(())
}

// Compile a new WebAssembly module.
//
// The `spawn` function can be used to spawn new processes from the module.
// Module compilation can be a CPU intensive task.
//
// Returns:
// *  0 on success - The ID of the newly created module is written to **id_ptr**
// *  1 on error   - The error ID is written to **id_ptr**
// * -1 in case the process doesn't have permission to compile modules.
fn compile_module<T>(
    mut caller: Caller<T>,
    module_data_ptr: u32,
    module_data_len: u32,
    id_ptr: u32,
) -> Result<i32, Trap>
where
    T: ProcessState + ProcessCtx<T> + ErrorCtx,
    T::Config: ProcessConfigCtx,
{
    // TODO: Module compilation is CPU intensive and should be done on the blocking task thread pool.
    if !caller.data().config().can_compile_modules() {
        return Ok(-1);
    }

    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.process.modules.compiled");

    #[cfg(feature = "metrics")]
    metrics::increment_gauge!("lunatic.process.modules.active", 1.0);

    let start = Instant::now();

    let mut module = vec![0; module_data_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, module_data_ptr as usize, module.as_mut_slice())
        .or_trap("lunatic::process::compile_module")?;

    let module = RawWasm::new(None, module);
    let (mod_or_error_id, result) = match caller.data().runtime().compile_module(module) {
        Ok(module) => (
            caller
                .data_mut()
                .module_resources_mut()
                .add(Arc::new(module)),
            0,
        ),
        Err(error) => (caller.data_mut().error_resources_mut().add(error), 1),
    };

    #[cfg(feature = "metrics")]
    let duration = Instant::now() - start;
    #[cfg(feature = "metrics")]
    metrics::histogram!("lunatic.process.modules.compiled.duration", duration);

    memory
        .write(&mut caller, id_ptr as usize, &mod_or_error_id.to_le_bytes())
        .or_trap("lunatic::process::compile_module")?;
    Ok(result)
}

// Drops the module from resources.
//
// Traps:
// * If the module ID doesn't exist.
fn drop_module<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    module_id: u64,
) -> Result<(), Trap> {
    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.process.modules.dropped");

    #[cfg(feature = "metrics")]
    metrics::decrement_gauge!("lunatic.process.modules.active", 1.0);

    caller
        .data_mut()
        .module_resources_mut()
        .remove(module_id)
        .or_trap("lunatic::process::drop_module: Module ID doesn't exist")?;
    Ok(())
}

// Create a new configuration with all permissions denied.
//
// There is no memory or fuel limit set on the newly created configuration.
//
// Returns:
// * ID of newly created configuration in case of success
// * -1 in case the process doesn't have permission to create new configurations
fn create_config<T>(mut caller: Caller<T>) -> i64
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    if !caller.data().config().can_create_configs() {
        return -1;
    }
    let config = T::Config::default();
    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.process.configs.created");
    #[cfg(feature = "metrics")]
    metrics::increment_gauge!("lunatic.process.configs.active", 1.0);
    caller.data_mut().config_resources_mut().add(config) as i64
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
    #[cfg(feature = "metrics")]
    metrics::increment_counter!("lunatic.process.configs.dropped");
    #[cfg(feature = "metrics")]
    metrics::decrement_gauge!("lunatic.process.configs.active", 1.0);
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
    caller: Caller<T>,
    config_id: u64,
) -> Result<u64, Trap> {
    let max_memory = caller
        .data()
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
    caller: Caller<T>,
    config_id: u64,
) -> Result<u64, Trap> {
    let max_fuel = caller
        .data()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_get_max_fuel: Config ID doesn't exist")?
        .get_max_fuel();
    match max_fuel {
        None => Ok(0),
        Some(max_fuel) => Ok(max_fuel),
    }
}

// Returns 1 if processes spawned from this configuration can compile Wasm modules, otherwise 0.
//
// Traps:
// * If the config ID doesn't exist.
fn config_can_compile_modules<T>(caller: Caller<T>, config_id: u64) -> Result<u32, Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    let can = caller
        .data()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_can_compile_modules: Config ID doesn't exist")?
        .can_compile_modules();
    Ok(can as u32)
}

// If set to a value >0 (true), processes spawned from this configuration will be able to compile
// Wasm modules.
//
// Traps:
// * If the config ID doesn't exist.
fn config_set_can_compile_modules<T>(
    mut caller: Caller<T>,
    config_id: u64,
    can: u32,
) -> Result<(), Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    caller
        .data_mut()
        .config_resources_mut()
        .get_mut(config_id)
        .or_trap("lunatic::process::config_set_can_compile_modules: Config ID doesn't exist")?
        .set_can_compile_modules(can != 0);
    Ok(())
}

// Returns 1 if processes spawned from this configuration can create other configurations,
// otherwise 0.
//
// Traps:
// * If the config ID doesn't exist.
fn config_can_create_configs<T>(caller: Caller<T>, config_id: u64) -> Result<u32, Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    let can = caller
        .data()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_can_create_configs: Config ID doesn't exist")?
        .can_create_configs();
    Ok(can as u32)
}

// If set to a value >0 (true), processes spawned from this configuration will be able to create
// other configuration.
//
// Traps:
// * If the config ID doesn't exist.
fn config_set_can_create_configs<T>(
    mut caller: Caller<T>,
    config_id: u64,
    can: u32,
) -> Result<(), Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    caller
        .data_mut()
        .config_resources_mut()
        .get_mut(config_id)
        .or_trap("lunatic::process::config_set_can_create_configs: Config ID doesn't exist")?
        .set_can_create_configs(can != 0);
    Ok(())
}

// Returns 1 if processes spawned from this configuration can spawn sub-processes, otherwise 0.
//
// Traps:
// * If the config ID doesn't exist.
fn config_can_spawn_processes<T>(caller: Caller<T>, config_id: u64) -> Result<u32, Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    let can = caller
        .data()
        .config_resources()
        .get(config_id)
        .or_trap("lunatic::process::config_can_spawn_processes: Config ID doesn't exist")?
        .can_spawn_processes();
    Ok(can as u32)
}

// If set to a value >0 (true), processes spawned from this configuration will be able to spawn
// sub-processes.
//
// Traps:
// * If the config ID doesn't exist.
fn config_set_can_spawn_processes<T>(
    mut caller: Caller<T>,
    config_id: u64,
    can: u32,
) -> Result<(), Trap>
where
    T: ProcessState + ProcessCtx<T>,
    T::Config: ProcessConfigCtx,
{
    caller
        .data_mut()
        .config_resources_mut()
        .get_mut(config_id)
        .or_trap("lunatic::process::config_set_can_spawn_processes: Config ID doesn't exist")?
        .set_can_spawn_processes(can != 0);
    Ok(())
}

// Spawns a new process using the passed in function inside a module as the entry point.
//
// If **link** is not 0, it will link the child and parent processes. The value of the **link**
// argument will be used as the link-tag for the child. This means, if the child traps the parent
// is going to get a signal back with the value used as the tag.
//
// If *config_id* or *module_id* have the value -1, the same module/config is used as in the
// process calling this function.
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
// * If the module ID doesn't exist.
// * If the function string is not a valid utf8 string.
// * If the params array is in a wrong format.
// * If any memory outside the guest heap space is referenced.
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
    T: ProcessState + ProcessCtx<T> + ErrorCtx + LunaticWasiCtx + ResourceLimiter + Send + 'static,
    for<'a> &'a T: Send,
    T::Config: ProcessConfigCtx,
{
    Box::new(async move {
        if !caller.data().config().can_spawn_processes() {
            return Err(anyhow!("Process doesn't have permissions to spawn sub-processes").into());
        }

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
                .or_trap("lunatic::process::spawn: Module ID doesn't exist")?
                .clone(),
        };

        let mut state = state.new_state(module.clone(), config)?;

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
        let params_chunks = &mut params.chunks_exact(17);
        let params = params_chunks
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
        if !params_chunks.remainder().is_empty() {
            return Err(anyhow!(
                "Params array must be in chunks of 17 bytes, but {} bytes remained",
                params_chunks.remainder().len()
            )
            .into());
        }
        // Should processes be linked together?
        let link: Option<(Option<i64>, Arc<dyn Process>)> = match link {
            0 => None,
            tag => {
                let id = caller.data().id();
                let signal_mailbox = caller.data().signal_mailbox().clone();
                let process = WasmProcess::new(id, signal_mailbox.0);
                Some((Some(tag), Arc::new(process)))
            }
        };

        let runtime = caller.data().runtime().clone();

        // Inherit stdout and stderr streams if they are redirected by the parent.
        let stdout = if let Some(stdout) = caller.data().get_stdout() {
            let next_stream = stdout.next();
            state.set_stdout(next_stream.clone());
            Some((stdout.clone(), next_stream))
        } else {
            None
        };
        if let Some(stderr) = caller.data().get_stderr() {
            // If stderr is same as stdout, use same `next_stream`.
            if let Some((stdout, next_stream)) = stdout {
                if &stdout == stderr {
                    state.set_stderr(next_stream);
                } else {
                    state.set_stderr(stderr.next());
                }
            } else {
                state.set_stderr(stderr.next());
            }
        }

        // set state instead of config TODO
        let env = caller.data().environment();
        let (proc_or_error_id, result) = match lunatic_process::wasm::spawn_wasm(
            env, runtime, &module, state, function, params, link,
        )
        .await
        {
            Ok((_, process)) => (process.id(), 0),
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

// lunatic::process::sleep_ms(millis: u64)
//
// Suspend process for `millis`.
fn sleep_ms<T: ProcessState + ProcessCtx<T>>(
    _: Caller<T>,
    millis: u64,
) -> Box<dyn Future<Output = ()> + Send + '_> {
    Box::new(async move {
        tokio::time::sleep(Duration::from_millis(millis)).await;
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
        .0
        .send(Signal::DieWhenLinkDies(trap != 0))
        .expect("The signal is sent to itself and the receiver must exist at this point");
}

// Returns ID of the process currently running
fn process_id<T: ProcessState + ProcessCtx<T>>(caller: Caller<T>) -> u64 {
    caller.data().id()
}

// Returns ID of the environment in which the process is currently running
fn environment_id<T: ProcessState + ProcessCtx<T>>(caller: Caller<T>) -> u64 {
    caller.data().environment().id()
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
    let this_process = WasmProcess::new(id, signal_mailbox.0);

    // Send link signal to other process
    let process = caller.data().environment().get_process(process_id);

    if let Some(process) = process {
        process.send(Signal::Link(tag, Arc::new(this_process)));

        // Send link signal to itself
        caller
            .data_mut()
            .signal_mailbox()
            .0
            .send(Signal::Link(tag, process))
            .expect("The Link signal is sent to itself and the receiver must exist at this point");
    } else {
        caller
            .data_mut()
            .signal_mailbox()
            .0
            .send(Signal::LinkDied(process_id, tag, DeathReason::NoProcess))
            .expect(
                "The LinkDied signal is sent to itself and the receiver must exist at this point",
            );
    }
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
    let this_process_id = caller.data().id();

    // Send unlink signal to other process
    let process = caller.data().environment().get_process(process_id);

    if let Some(process) = process {
        process.send(Signal::UnLink {
            process_id: this_process_id,
        });
    }

    // Send unlink signal to itself
    caller
        .data_mut()
        .signal_mailbox()
        .0
        .send(Signal::UnLink { process_id })
        .expect("The signal is sent to itself and the receiver must exist at this point");

    Ok(())
}

// Send a Kill signal to **process_id**.
//
// Traps:
// * If the process ID doesn't exist.
fn kill<T: ProcessState + ProcessCtx<T>>(caller: Caller<T>, process_id: u64) -> Result<(), Trap> {
    // Send kill signal to process
    if let Some(process) = caller.data().environment().get_process(process_id) {
        process.send(Signal::Kill);
    }
    Ok(())
}

// Checks to see if a process exists
fn exists<T: ProcessState + ProcessCtx<T>>(caller: Caller<T>, process_id: u64) -> i32 {
    caller.data().environment().get_process(process_id).is_some() as i32
}
