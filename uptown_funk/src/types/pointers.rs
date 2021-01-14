use std::marker::PhantomData;

use crate::{memory::Memory, Executor, FromWasm};

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
    pub fn copy_slice(self, slice: &[u8]) -> Option<Self> {
        let loc = self.loc + slice.len();
        if loc > self.mem.as_mut_slice().len() {
            None
        } else {
            self.mem.as_mut_slice()[self.loc..self.loc + slice.len()].copy_from_slice(slice);

            if loc == self.mem.as_mut_slice().len() {
                return None;
            }

            Some(Self { loc, ..self })
        }
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
