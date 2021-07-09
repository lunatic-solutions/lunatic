use anyhow::Result;
use wasmtime::{Caller, Linker, Trap};

use super::{get_memory, link_if_match};
use crate::{api::error::IntoTrap, state::State};

// Register the plugin APIs to the linker
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &[String]) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::plugin",
        "take_module",
        take_module,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::plugin",
        "set_module",
        set_module,
        namespace_filter,
    )?;
    Ok(())
}

//% lunatic::plugin::take_module(offset: i32)
//%
//% Takes the Wasm binary that is being compiled and writes it to the guest memory at **offset**.
//%
//% It's intended to be used from plugins inside the `lunatic_create_module_hook` hook to modify
//% the Wasm modules before they are created. Once taken (and modified), the new module needs to
//% be set back with `lunatic::plugin::set_module` to make it available for other plugins that
//% may modify it. The call to `lunatic_create_module_hook` will pass the total size of the Wasm
//% binary to the plugin, so it can upfront reserve enough memory.
//%
//% Traps:
//% * If it's called outside of a `lunatic_create_module_hook` hook.
//% * If a plugin that was called before took this module, but didn't put it back.
//% * If there is not enough space in memory at **offset** to write the whole binary.
fn take_module(mut caller: Caller<State>, offset: u32) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;
    let module = caller
        .data_mut()
        .module_loaded
        .take()
        .or_trap("lunatic::plugin::take_module")?;
    memory
        .write(&mut caller, offset as usize, module.as_slice())
        .or_trap("lunatic::plugin::take_module")?;
    Ok(())
}

//% lunatic::plugin::set_module(offset: i32, offset_len: i32)
//%
//% Updates the Wasm binary that is being compiled with a new one provided by the plugin.
//%
//% It's intended to be used from plugins inside the `lunatic_create_module_hook` hook to modify
//% the Wasm modules before they are created.
//%
//% Traps:
//% * If it's called outside of a `lunatic_create_module_hook` hook.
//% * If **offset + offset_len** is outside the plugin's memory.
fn set_module(mut caller: Caller<State>, offset: u32, offset_len: u32) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;
    let mut new_module = vec![0; offset_len as usize];
    memory
        .read(&caller, offset as usize, new_module.as_mut_slice())
        .or_trap("lunatic::plugin::set_module")?;
    caller.data_mut().module_loaded = Some(new_module);
    Ok(())
}
