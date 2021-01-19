use std::marker::PhantomData;

use crate::{memory::Memory, Executor, FromWasm, Trap};

pub trait WasmType {
    type Value;
    fn copy_to(&self, mem: &mut [u8]);
    fn len() -> usize;
    fn value_from_memory(mem: &[u8]) -> Self::Value;
}

impl WasmType for u8 {
    type Value = u8;

    fn copy_to(&self, mem: &mut [u8]) {
        mem[..1].copy_from_slice(&self.to_le_bytes());
    }

    #[inline]
    fn len() -> usize {
        1
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        mem[0]
    }
}

impl WasmType for u32 {
    type Value = u32;

    fn copy_to(&self, mem: &mut [u8]) {
        mem[..4].copy_from_slice(&self.to_le_bytes());
    }

    #[inline]
    fn len() -> usize {
        4
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        u32::from_le_bytes([mem[0], mem[1], mem[2], mem[3]])
    }
}

impl WasmType for u64 {
    type Value = u64;

    fn copy_to(&self, mem: &mut [u8]) {
        mem[..8].copy_from_slice(&self.to_le_bytes());
    }

    #[inline]
    fn len() -> usize {
        8
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        u64::from_le_bytes([mem[0], mem[1], mem[2], mem[3], mem[4], mem[5], mem[6], mem[7]])
    }
}

impl WasmType for f64 {
    type Value = f64;

    fn copy_to(&self, mem: &mut [u8]) {
        mem[..8].copy_from_slice(&self.to_le_bytes());
    }

    #[inline]
    fn len() -> usize {
        8
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        f64::from_le_bytes([
            mem[0], mem[1], mem[2], mem[3], mem[4], mem[5], mem[6], mem[7],
        ])
    }
}

impl<S, T: WasmType> WasmType for Pointer<S, T> {
    type Value = T::Value;

    fn copy_to(&self, mem: &mut [u8]) {
        mem[..4].copy_from_slice(&(self.loc as u32).to_le_bytes());
    }

    #[inline]
    fn len() -> usize {
        4
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        T::value_from_memory(mem)
    }
}

pub struct Pointer<S, T: WasmType> {
    loc: usize,
    mem: Memory,
    _state: PhantomData<S>,
    _type: PhantomData<T>,
}

impl<S, T: WasmType> Pointer<S, T> {
    pub fn set(&mut self, val: &T) {
        val.copy_to(&mut self.mem.as_mut_slice()[(self.loc as usize)..]);
    }

    pub fn value(&self) -> T::Value {
        T::value_from_memory(&self.mem.as_mut_slice()[self.loc..])
    }

    pub fn next(self) -> Option<Self> {
        let loc = self.loc + T::len();
        if loc >= self.mem.as_mut_slice().len() {
            None
        } else {
            Some(Self { loc, ..self })
        }
    }
}

impl<S> Pointer<S, u8> {
    pub fn copy_slice(self, slice: &[u8]) -> Result<Option<Self>, Trap> {
        let loc = self.loc + slice.len();
        if loc > self.mem.as_mut_slice().len() {
            Err(Trap::new("Tried to copy slice over memory bounds"))
        } else {
            self.mem.as_mut_slice()[self.loc..self.loc + slice.len()].copy_from_slice(slice);

            if loc == self.mem.as_mut_slice().len() {
                return Ok(None);
            }

            Ok(Some(Self { loc, ..self }))
        }
    }

    pub fn mut_slice<'a>(&'a self, n: usize) -> &'a mut [u8] {
        let slice = &mut self.mem.as_mut_slice()[self.loc..self.loc + n];
        slice
    }
}

impl<S, T: WasmType> FromWasm for Pointer<S, T> {
    type From = u32;
    type State = S;

    fn from(
        _state: &mut Self::State,
        executor: &impl Executor,
        wasm_u32: u32,
    ) -> Result<Self, crate::Trap> {
        Ok(Pointer {
            loc: wasm_u32 as usize,
            mem: executor.memory(),
            _state: PhantomData::default(),
            _type: PhantomData::default(),
        })
    }
}
