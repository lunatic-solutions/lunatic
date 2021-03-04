use std::{cell::Cell, ptr};

#[cfg(feature = "vm-wasmtime")]
type CallThreadState = wasmtime_runtime::traphandlers::CallThreadState<'static>;
#[cfg(feature = "vm-wasmer")]
type CallThreadState = wasmer_vm::traphandlers::CallThreadState;

#[derive(Clone)]
pub struct CallThreadStateSave {
    saved: Cell<*const CallThreadState>,
}

unsafe impl Send for CallThreadStateSave {}

impl CallThreadStateSave {
    pub fn new() -> Self {
        Self {
            saved: Cell::new(ptr::null()),
        }
    }

    pub fn swap(&self) {
        #[cfg(feature = "vm-wasmtime")]
        wasmtime_runtime::traphandlers::tls::PTR.with(|cell| cell.swap(&self.saved));
        #[cfg(feature = "vm-wasmer")]
        wasmer_vm::traphandlers::tls::PTR.with(|cell| cell.swap(&self.saved));
    }
}
