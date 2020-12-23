use super::types::*;

use anyhow::Result;
use uptown_funk::{host_functions, FromWasmI32};

use log::trace;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};

pub struct WasiState {}

impl WasiState {
    pub fn new() -> Self {
        Self {}
    }
}

struct ExitCode {}

impl FromWasmI32 for ExitCode {
    type State = WasiState;

    fn from_i32<I>(
        _state: &Self::State,
        _instance_environment: &I,
        exit_code: i32,
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

#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    fn proc_exit(&self, _exit_code: ExitCode) {}

    fn fd_write(&self, fd: i32, ciovs: &[IoSlice<'_>]) -> (i32, i32) {
        match fd {
            // Stdin not supported as write destination
            0 => (WASI_EINVAL, 0),
            1 => {
                let written = io::stdout().write_vectored(ciovs).unwrap();
                (WASI_ESUCCESS, written as i32)
            }
            2 => {
                let written = io::stderr().write_vectored(ciovs).unwrap();
                (WASI_ESUCCESS, written as i32)
            }
            _ => panic!("Unsupported wasi write destination"),
        }
    }

    fn fd_read(&self, fd: i32, iovs: &mut [IoSliceMut<'_>]) -> (i32, i32) {
        match fd {
            // Stdout & stderr not supported as read destination
            1 | 2 => (WASI_EINVAL, 0),
            0 => {
                let written = io::stdin().read_vectored(iovs).unwrap();
                (WASI_ESUCCESS, written as i32)
            }
            _ => panic!("Unsupported wasi read destination"),
        }
    }

    fn path_open(
        &self,
        _a: i32,
        _b: i32,
        _c: i32,
        _d: i32,
        _e: i32,
        _f: i64,
        _g: i64,
        _h: i32,
    ) -> (i32, i32) {
        (0, 0)
    }

    fn fd_close(&self, fd: i32) -> i32 {
        trace!("wasi_snapshot_preview1:fd_close({})", fd);
        WASI_ESUCCESS
    }

    fn fd_prestat_get(&self, _fd: i32, _prestat_ptr: i32) -> i32 {
        WASI_EBADF
    }

    fn fd_prestat_dir_name(&self, _fd: i32, _path: &str) -> i32 {
        WASI_ESUCCESS
    }

    fn environ_sizes_get(&self, _environ: i32) -> (i32, i32) {
        (WASI_ESUCCESS, 0)
    }

    fn environ_get(&self, _environ: i32) -> (i32, i32) {
        (WASI_ESUCCESS, 0)
    }
}
