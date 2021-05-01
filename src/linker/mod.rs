#[cfg(feature = "vm-wasmtime")]
mod wasmtime;
#[cfg(feature = "vm-wasmtime")]
pub use self::wasmtime::{engine as wasmtime_engine, LunaticLinker as WasmtimeLunaticLinker};
