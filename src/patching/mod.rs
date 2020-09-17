use walrus::Module;
use anyhow::Error;

mod reduction_counting;

/// Prepares a WASM module before execution inside of Lunatic. This preparation includes:
/// * Adding reduction counters to functions and ~hot loops~.
pub fn patch(module_buffer: &[u8]) -> Result<Vec<u8>, Error> {
    let mut module = Module::from_buffer(&module_buffer)?;

    reduction_counting::patch(&mut module);

    Ok(module.emit_wasm())
}