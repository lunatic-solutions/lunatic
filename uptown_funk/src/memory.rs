#[derive(Clone)]
pub enum Memory {
    Wasmer(wasmer::Memory),
    Wasmtime(wasmtime::Memory),
}

impl Memory {
    pub fn from<M: Into<Memory>>(memory: M) -> Self {
        memory.into()
    }

    pub fn as_mut_slice(&self) -> &mut [u8] {
        unsafe {
            match self {
                Memory::Wasmer(mem) => mem.data_unchecked_mut(),
                Memory::Wasmtime(mem) => mem.data_unchecked_mut(),
            }
        }
    }
}

impl Into<Memory> for wasmer::Memory {
    fn into(self) -> Memory {
        Memory::Wasmer(self)
    }
}

impl Into<Memory> for wasmtime::Memory {
    fn into(self) -> Memory {
        Memory::Wasmtime(self)
    }
}
