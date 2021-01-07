use std::mem::ManuallyDrop;

pub trait LunaticMemory {
    fn slice_mut(&self) -> &mut [u8];
}

#[cfg(feature = "vm-wasmtime")]
impl LunaticMemory for ManuallyDrop<wasmtime::Memory> {
    fn slice_mut(&self) -> &mut [u8] {
        unsafe { self.data_unchecked_mut() }
    }
}
