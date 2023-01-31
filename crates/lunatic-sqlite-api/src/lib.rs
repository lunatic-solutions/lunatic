pub mod wire_format;

#[cfg(not(target_arch = "wasm32"))]
mod sqlite_bindings;

#[cfg(not(target_arch = "wasm32"))]
pub use sqlite_bindings::*;

#[cfg(target_arch = "wasm32")]
pub mod guest_api;
