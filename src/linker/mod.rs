#[cfg(feature = "vm-wasmtime")]
mod wasmtime;
#[cfg(feature = "vm-wasmtime")]
pub use self::wasmtime::*;

#[cfg(feature = "vm-wasmer")]
mod wasmer;
#[cfg(feature = "vm-wasmer")]
pub use self::wasmer::*;
