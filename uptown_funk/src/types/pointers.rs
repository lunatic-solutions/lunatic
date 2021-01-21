use std::{cell::Cell, marker::PhantomData, mem};

use crate::{Executor, FromWasm, StateMarker, Trap, memory::Memory};

pub trait WasmType : Sized {
    type Value;
    fn copy_to(&self, mem: &mut [u8]);
    fn len() -> usize;
    fn value_from_memory(mem: &[u8]) -> Self::Value;

    fn move_to(self, mem: &mut [u8]) {
        self.copy_to(mem);
    }
}

pub trait CReprWasmType : Sized {}

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

impl CReprWasmType for u32 {}
impl CReprWasmType for u64 {}
impl CReprWasmType for f64 {}
impl CReprWasmType for f32 {}

pub struct Pointer<T: WasmType> {
    loc: usize,
    mem: Memory,
    _type: PhantomData<T>,
}

impl<T: WasmType> WasmType for Pointer<T> {
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

impl<T: WasmType> Pointer<T> {
    pub fn copy(&mut self, val: &T) {
        val.copy_to(&mut self.mem.as_mut_slice()[(self.loc as usize)..]);
    }

    pub fn set(&mut self, val: T) {
        val.move_to(&mut self.mem.as_mut_slice()[(self.loc as usize)..]);
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

impl Pointer<u8> {
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

impl<T: WasmType> FromWasm for Pointer<T> {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        executor: &impl Executor,
        wasm_u32: u32,
    ) -> Result<Self, crate::Trap> {
        Ok(Pointer {
            loc: wasm_u32 as usize,
            mem: executor.memory(),
            _type: PhantomData::default(),
        })
    }
}

fn align_pointer(ptr: usize, align: usize) -> usize {
    // clears bits below aligment amount (assumes power of 2) to align pointer
    ptr & !(align - 1)
}

fn deref<T: Sized>(offset: u32, memory: &[u8], index: u32, length: u32) -> Option<&[Cell<T>]> {
    // gets the size of the item in the array with padding added such that
    // for any index, we will always result an aligned memory access
    let item_size = mem::size_of::<T>() + (mem::size_of::<T>() % mem::align_of::<T>());
    let slice_full_len = index as usize + length as usize;
    let memory_size = memory.len();

    if (offset as usize) + (item_size * slice_full_len) > memory_size
        || offset as usize >= memory_size
        || mem::size_of::<T>() == 0
    {
        return None;
    }

    unsafe {
        let cell_ptr = align_pointer(
            memory.as_ptr().add(offset as usize) as usize,
            mem::align_of::<T>(),
        ) as *const Cell<T>;
        let cell_ptrs = &std::slice::from_raw_parts(cell_ptr, slice_full_len)
            [index as usize..slice_full_len];
        Some(cell_ptrs)
    }
}

impl <T: CReprWasmType + Copy + Clone> WasmType for T {
    type Value = T;

    fn copy_to(&self, mem: &mut [u8]) {
        self.clone().move_to(mem);
    }

    fn move_to(self, mem: &mut [u8]) {
        // TODO what if it fails?
        if let Some(cells) = deref::<T>(0, mem, 0, 1) { 
            cells[0].set(self);
        }
    }

    fn len() -> usize {
        mem::size_of::<T>()
    }

    fn value_from_memory(mem: &[u8]) -> Self::Value {
        // TODO unwrap
        let cells = deref::<T>(0, mem, 0, 1).unwrap();
        cells[0].get()
    }
}

//impl<S, T : CReprWasmType> Pointer<S, T> {
//    pub fn set_c_repr_type(&self, val: T) {
//    }
//}