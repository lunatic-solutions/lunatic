//! WebAssembly runtimes powering lunatic.
//!
//! Currently only Wasmtime is supported, but it should be "easy" to add any runtime that has a
//! `Linker` abstraction and supports `async` host functions.
//!
//! NOTE: This traits are not used at all. Until rust supports async-traits all functions working
//!       with a runtime will directly take `wasmtime::WasmtimeRuntime` instead of a generic.

pub mod wasmtime;

pub type RawWasm = Vec<u8>;

/// A `WasmRuntime` is a compiler that can generate runnable code from raw .wasm files.
///
/// It also provides a mechanism to register host functions that are accessible to the wasm guest
/// code through the generic type `T`. The type `T` must implement the [`ProcessState`] trait and
/// expose a `register` function for host functions.
pub trait WasmRuntime<T>: Clone
where
    T: crate::state::ProcessState + Default + Send,
{
    type WasmInstance: WasmInstance;

    /// Takes a raw binary WebAssembly module and returns the index of a compiled module.
    fn compile_module(&mut self, data: RawWasm) -> anyhow::Result<usize>;

    /// Returns a reference to the raw binary WebAssembly module if the index exists.
    fn wasm_module(&self, index: usize) -> Option<&RawWasm>;

    // Creates a wasm instance from compiled module if the index exists.
    /* async fn instantiate(
        &self,
        index: usize,
        state: T,
        config: ProcessConfig,
    ) -> Result<WasmtimeInstance<T>>; */
}

pub trait WasmInstance {
    type Param;

    // Calls a wasm function by name with the specified arguments. Ignores the returned values.
    /* async fn call(&mut self, function: &str, params: Vec<Self::Param>) -> Result<()>; */
}
