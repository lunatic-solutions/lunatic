pub struct CallThreadStateSaveWasmtime {
    saved: Option<wasmtime_runtime::TlsRestore>,
    init: bool,
}

impl CallThreadStateSaveWasmtime {
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

unsafe impl Send for CallThreadStateSaveWasmtime {}
