use super::types::*;

use anyhow::Result;
use uptown_funk::{host_functions, FromWasmU32};

use log::trace;
use std::{
    io::{self, IoSlice, IoSliceMut, Read, Write},
    marker::PhantomData,
};

lazy_static::lazy_static! {
    static ref ENV : WasiEnvVars = WasiEnvVars::new(std::env::vars());
}

pub struct WasiState {}

impl WasiState {
    pub fn new() -> Self {
        Self {}
    }
}

trait WasmType {
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

struct Pointer<'a, S, T: WasmType> {
    loc: usize,
    mem: &'a mut [u8],
    _state: PhantomData<S>,
    _type: PhantomData<T>,
}

impl<'a, S, T: WasmType> Pointer<'a, S, T> {
    fn set(&mut self, val: &T) {
        val.copy_to(&mut self.mem[(self.loc as usize)..]);
    }

    fn value(&self) -> T::Value {
        T::value_from_memory(&self.mem[self.loc..])
    }

    fn next(self) -> Option<Self> {
        let loc = self.loc + T::len();
        if loc >= self.mem.len() {
            None
        } else {
            Some(Self { loc, ..self })
        }
    }
}

impl<'a, S> Pointer<'a, S, u8> {
    fn copy_slice(self, slice: &[u8]) -> Option<Self> {
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
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
        I: uptown_funk::InstanceEnvironment,
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

struct ExitCode {}

impl<'a> FromWasmU32<'a> for ExitCode {
    type State = WasiState;

    fn from_u32<I>(
        _state: &mut Self::State,
        _instance_environment: &'a I,
        exit_code: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
        I: uptown_funk::InstanceEnvironment,
    {
        Err(uptown_funk::Trap::new(format!(
            "proc_exit({}) called",
            exit_code
        )))
    }
}

type Ptr<'a, T> = Pointer<'a, WasiState, T>;

#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    fn proc_exit(&self, _exit_code: ExitCode) {}

    fn fd_write(&self, fd: u32, ciovs: &[IoSlice<'_>]) -> (u32, u32) {
        match fd {
            // Stdin not supported as write destination
            0 => (WASI_EINVAL, 0),
            1 => {
                let written = io::stdout().write_vectored(ciovs).unwrap();
                (WASI_ESUCCESS, written as u32)
            }
            2 => {
                let written = io::stderr().write_vectored(ciovs).unwrap();
                (WASI_ESUCCESS, written as u32)
            }
            _ => panic!("Unsupported wasi write destination"),
        }
    }

    fn fd_read(&self, fd: u32, iovs: &mut [IoSliceMut<'_>]) -> (u32, u32) {
        match fd {
            // Stdout & stderr not supported as read destination
            1 | 2 => (WASI_EINVAL, 0),
            0 => {
                let written = io::stdin().read_vectored(iovs).unwrap();
                (WASI_ESUCCESS, written as u32)
            }
            _ => panic!("Unsupported wasi read destination"),
        }
    }

    fn path_open(
        &self,
        _a: u32,
        _b: u32,
        _c: u32,
        _d: u32,
        _e: u32,
        _f: i64,
        _g: i64,
        _h: u32,
    ) -> (u32, u32) {
        (0, 0)
    }

    fn fd_close(&self, fd: u32) -> u32 {
        trace!("wasi_snapshot_preview1:fd_close({})", fd);
        WASI_ESUCCESS
    }

    fn fd_prestat_get(&self, _fd: u32, _prestat_ptr: u32) -> u32 {
        WASI_EBADF
    }

    fn fd_prestat_dir_name(&self, _fd: u32, _path: &str) -> u32 {
        WASI_ESUCCESS
    }

    fn environ_sizes_get(&self, mut var_count: Ptr<u32>, mut total_bytes: Ptr<u32>) -> u32 {
        var_count.set(&ENV.len());
        total_bytes.set(&ENV.total_bytes());
        WASI_ESUCCESS
    }

    fn environ_get<'a>(&self, mut environ: Ptr<Ptr<'a, u8>>, mut environ_buf: Ptr<'a, u8>) -> u32 {
        for kv in ENV.iter() {
            environ.set(&environ_buf);
            environ_buf = environ_buf.copy_slice(&kv).unwrap();
            environ = environ.next().unwrap();
        }
        WASI_ESUCCESS
    }
}
