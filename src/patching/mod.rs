use walrus::Module;
use anyhow::Error;

mod reduction_counting;
mod stdlib;
mod shared_memory;

/// Prepares a WASM module before execution inside of Lunatic. This preparation includes:
/// * Adding reduction counters to functions and ~hot loops~.
pub fn patch(module_buffer: &[u8]) -> Result<(u32, Vec<u8>), Error> {
    let mut module = Module::from_buffer(&module_buffer)?;

    reduction_counting::patch(&mut module);
    stdlib::patch(&mut module);
    let min_memory = shared_memory::patch(&mut module);

    Ok((min_memory, module.emit_wasm()))
}