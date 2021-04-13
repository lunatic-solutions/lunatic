#[cfg(feature = "vm-wasmtime")]
mod wasmtime;
#[cfg(feature = "vm-wasmtime")]
pub use self::wasmtime::{engine as wasmtime_engine, LunaticLinker as WasmtimeLunaticLinker};

#[cfg(feature = "vm-wasmer")]
mod wasmer;
#[cfg(feature = "vm-wasmer")]
pub use self::wasmer::{engine as wasmer_engine, LunaticLinker as WasmerLunaticLinker};
