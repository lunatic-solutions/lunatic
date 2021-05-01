#[derive(Clone)]
pub enum Memory {
    Empty,
    #[cfg(feature = "vm-wasmtime")]
    Wasmtime(wasmtime::Memory),
}

impl Memory {
    pub fn from<M: Into<Memory>>(memory: M) -> Self {
        memory.into()
    }

    pub fn as_mut_slice(&self) -> &mut [u8] {
        unsafe {
            match self {
                #[cfg(feature = "vm-wasmtime")]
                Memory::Wasmtime(mem) => mem.data_unchecked_mut(),
                Memory::Empty => panic!("Called as_mut_slice() on uptown_funk::Memory::Empty"),
            }
        }
    }
}

#[cfg(feature = "vm-wasmtime")]
impl Into<Memory> for wasmtime::Memory {
    fn into(self) -> Memory {
        Memory::Wasmtime(self)
    }
}
