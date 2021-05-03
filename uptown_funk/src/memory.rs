#[derive(Clone)]
pub enum Memory {
    Empty,
    Wasmtime(wasmtime::Memory),
}

impl Memory {
    pub fn from<M: Into<Memory>>(memory: M) -> Self {
        memory.into()
    }

    pub fn as_mut_slice(&self) -> &mut [u8] {
        unsafe {
            match self {
                Memory::Wasmtime(mem) => mem.data_unchecked_mut(),
                Memory::Empty => panic!("Called as_mut_slice() on uptown_funk::Memory::Empty"),
            }
        }
    }
}

impl Into<Memory> for wasmtime::Memory {
    fn into(self) -> Memory {
        Memory::Wasmtime(self)
    }
}
