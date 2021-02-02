//! Each compiler that supports WebAssembly as a target (Rust, C, AssemblyScript, ...), will produce
//! slightly different WASM modules. It's impossible for Lunatic to provide just one interface that
//! *correctly* works with all the subtle abstraction differences chosen by each language. To solve
//! this issue we run a normalisation step on each WASM module before compiling it.
//!
//! This module contains a collection of patches that represent accumulated knowledge of specific
//! modifications, that need to be applied to a WASM module to correctly work when used from Lunatic.
//! This module will grow with time, as more languages are supported by Lunatic and more edge cases
//! are encountered.

use anyhow::Error;
use walrus::Module;

mod reduction_counting;
mod shared_memory;
mod stdlib;

/// Patches:
/// * Add reduction counters and yielding to functions and ~hot loops~.
/// * Add low level functions required by the Lunatic stdlib.
/// * Transforming defined memories into imported (shared) ones.
pub fn patch(module_buffer: &[u8]) -> Result<((u32, Option<u32>), Vec<u8>), Error> {
    let mut module = Module::from_buffer(&module_buffer)?;

    reduction_counting::patch(&mut module);
    stdlib::patch(&mut module)?;
    let memory = shared_memory::patch(&mut module);

    Ok((memory, module.emit_wasm()))
}
