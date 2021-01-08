use std::marker::PhantomData;

use crate::FromWasmU32;

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

impl<'a, S, T: WasmType> WasmType for Pointer<'a, S, T> {
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

pub struct Pointer<'a, S, T: WasmType> {
    loc: usize,
    mem: &'a mut [u8],
    _state: PhantomData<S>,
    _type: PhantomData<T>,
}

impl<'a, S, T: WasmType> Pointer<'a, S, T> {
    pub fn set(&mut self, val: &T) {
        val.copy_to(&mut self.mem[(self.loc as usize)..]);
    }

    pub fn value(&self) -> T::Value {
        T::value_from_memory(&self.mem[self.loc..])
    }

    pub fn next(self) -> Option<Self> {
        let loc = self.loc + T::len();
        if loc >= self.mem.len() {
            None
        } else {
            Some(Self { loc, ..self })
        }
    }
}

impl<'a, S> Pointer<'a, S, u8> {
    pub fn copy_slice(self, slice: &[u8]) -> Option<Self> {
        let loc = self.loc + slice.len();
        if loc > self.mem.len() {
            None
        } else {
            self.mem[self.loc..self.loc + slice.len()].copy_from_slice(slice);

            if loc == self.mem.len() {
                return None;
            }

            Some(Self { loc, ..self })
        }
    }
}

impl<'a, S, T: WasmType> FromWasmU32<'a> for Pointer<'a, S, T> {
    type State = S;

    fn from_u32<I>(
        _state: &mut Self::State,
        instance_environment: &'a I,
        wasm_u32: u32,
    ) -> Result<Self, crate::Trap>
    where
        Self: Sized,
        I: crate::InstanceEnvironment,
    {
        // TODO unwrap
        let mem = instance_environment.wasm_memory().get_mut(..).unwrap();
        Ok(Pointer {
            loc: wasm_u32 as usize,
            mem,
            _state: PhantomData::default(),
            _type: PhantomData::default(),
        })
    }
}