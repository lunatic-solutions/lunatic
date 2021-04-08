#[cfg(feature = "vm-wasmer")]
type CallThreadState = wasmer_vm::traphandlers::CallThreadState;

#[cfg(feature = "vm-wasmer")]
pub struct CallThreadStateSave {
    saved: std::cell::Cell<*const CallThreadState>,
}

unsafe impl Send for CallThreadStateSave {}

#[cfg(feature = "vm-wasmer")]
impl CallThreadStateSave {
    pub fn new() -> Self {
        Self {
            saved: std::cell::Cell::new(std::ptr::null()),
        }
    }

    pub fn swap(&self) {
        wasmer_vm::traphandlers::tls::PTR.with(|cell| cell.swap(&self.saved));
    }
}

#[cfg(feature = "vm-wasmtime")]
pub struct CallThreadStateSave {
    saved: Option<wasmtime_runtime::TlsRestore>,
    init: bool,
}

#[cfg(feature = "vm-wasmtime")]
impl CallThreadStateSave {
    pub fn new() -> Self {
        Self {
            saved: None,
            init: false,
        }
    }

    pub fn swap(&mut self) {
        // On first poll there is nothing to preserve yet.
        if self.init {
            unsafe {
                if let Some(tls) = self.saved.take() {
                    tls.replace()
                        .expect("wasmtime_runtime::sys::lazy_per_thread_init() failed");
                } else {
                    self.saved = Some(wasmtime_runtime::TlsRestore::take());
                }
            }
        } else {
            self.init = true;
        }
    }
}
