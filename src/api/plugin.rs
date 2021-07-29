use anyhow::Result;
use wasmtime::{Caller, Linker, Trap};

use super::get_memory;
use crate::{api::error::IntoTrap, state::PluginState};

// Register the plugin APIs to the linker.
// The plugin API is added as a whole to all plugins and doesn't require a filter.
pub(crate) fn register(linker: &mut Linker<PluginState>) -> Result<()> {
    linker.func_wrap("lunatic::plugin", "add_function", add_function)?;
    linker.func_wrap("lunatic::plugin", "add_function_type", add_function_type)?;
    linker.func_wrap(
        "lunatic::plugin",
        "add_function_export",
        add_function_export,
    )?;
    Ok(())
}

//% lunatic::plugin::add_function(
//%     type_index: u32,
//%     func_locals_ptr: u32,
//%     func_locals_len: u32,
//%     func_body_ptr: u32,
//%     func_body_len: u32,
//%     id_ptr: u32,
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The index of the newly created function is written to **id_ptr** as `u64`.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Adds function to the WebAssembly module.
//%
//% It's intended to be used from plugins inside of `lunatic_create_module_hook` to modify the Wasm
//% modules before they are created.
//%
//% Traps:
//% * If **func_local_ptr + func_local_len** is outside the plugin's memory.
//% * If **func_body_ptr + func_body_len** is outside the plugin's memory.
//% * If **id_ptr** is outside the memory.
fn add_function(
    mut caller: Caller<PluginState>,
    type_index: u32,
    func_locals_ptr: u32,
    func_locals_len: u32,
    func_body_ptr: u32,
    func_body_len: u32,
    id_ptr: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let mut func_locals = vec![0; func_locals_len as usize];
    memory
        .read(
            &caller,
            func_locals_ptr as usize,
            func_locals.as_mut_slice(),
        )
        .or_trap("lunatic::plugin::add_function")?;
    let mut func_body = vec![0; func_body_len as usize];
    memory
        .read(&caller, func_body_ptr as usize, func_body.as_mut_slice())
        .or_trap("lunatic::plugin::add_function")?;

    let module_context = caller.data_mut().module_context();
    let (id, result) = match module_context.add_function(type_index, &func_locals, func_body) {
        Ok(id) => (id, 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &id.to_le_bytes())
        .or_trap("lunatic::plugin::add_function")?;
    Ok(result)
}

//% lunatic::plugin::add_function_type(
//%     param_types_ptr: u32,
//%     param_types_len: u32,
//%     ret_types_ptr: u32,
//%     ret_types_len: u32,
//%     id_ptr: u32,
//% ) -> u32
//%
//% Returns:
//% * 0 on success - The index of the newly created type is written to **id_ptr** as `u64`.
//% * 1 on error   - The error ID is written to **id_ptr**
//%
//% Adds function type to the WebAssembly module.
//%
//% It's intended to be used from plugins inside of `lunatic_create_module_hook` to modify the Wasm
//% modules before they are created.
//%
//% Traps:
//% * If **param_types_ptr + param_types_len** is outside the plugin's memory.
//% * If **ret_types_ptr + ret_types_len** is outside the plugin's memory.
//% * If **id_ptr** is outside the memory.
fn add_function_type(
    mut caller: Caller<PluginState>,
    param_types_ptr: u32,
    param_types_len: u32,
    ret_types_ptr: u32,
    ret_types_len: u32,
    id_ptr: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let mut param_types = vec![0; param_types_len as usize];
    memory
        .read(
            &caller,
            param_types_ptr as usize,
            param_types.as_mut_slice(),
        )
        .or_trap("lunatic::plugin::add_function_type")?;
    let mut return_types = vec![0; ret_types_len as usize];
    memory
        .read(&caller, ret_types_ptr as usize, return_types.as_mut_slice())
        .or_trap("lunatic::plugin::add_function_type")?;

    let module_context = caller.data_mut().module_context();
    let (id, result) = match module_context.add_function_type(&param_types, &return_types) {
        Ok(id) => (id, 0),
        Err(error) => (caller.data_mut().errors.add(error), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(&mut caller, id_ptr as usize, &id.to_le_bytes())
        .or_trap("lunatic::plugin::add_function_type")?;
    Ok(result)
}

//% lunatic::plugin::add_function_export(name_str_ptr: u32, name_str_len: u32, function_id: u32)
//%
//% Adds function index as an export to the WebAssembly module.
//%
//% It's intended to be used from plugins inside of `lunatic_create_module_hook` to modify the Wasm
//% modules before they are created.
//%
//% Traps:
//% * If **name_str_ptr + name_str_len** is outside the plugin's memory.
fn add_function_export(
    mut caller: Caller<PluginState>,
    name_str_ptr: u32,
    name_str_len: u32,
    function_id: u32,
) -> Result<(), Trap> {
    let mut buffer = vec![0; name_str_len as usize];
    let memory = get_memory(&mut caller)?;
    memory
        .read(&caller, name_str_ptr as usize, buffer.as_mut_slice())
        .or_trap("lunatic::plugin::add_function_export")?;
    let name = String::from(
        std::str::from_utf8(buffer.as_slice()).or_trap("lunatic::plugin::add_function_export")?,
    );

    let module_context = caller.data_mut().module_context();
    module_context.add_function_export(name, function_id);
    Ok(())
}
