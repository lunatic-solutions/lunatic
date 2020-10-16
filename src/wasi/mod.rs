pub mod types;

use types::*;

use crate::process::ProcessEnvironment;
use anyhow::Result;
use wasmtime::{Linker, Trap};

use std::fmt;
use std::io::{stderr, stdout};

#[derive(Debug, Clone, Copy)]
struct ExitCode(i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExitCode {}

pub fn create_wasi_imports(
    linker: &mut Linker,
    process_env_original: &ProcessEnvironment,
) -> Result<()> {
    // proc_exit(exit_code)
    linker.func(
        "wasi_snapshot_preview1",
        "proc_exit",
        move |_exit_code: i32| -> Result<(), Trap> { Err(Trap::new("proc_exit() called")) },
    )?;

    // fd_write(...)
    let env = process_env_original.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "fd_write",
        move |fd: u32, iovs: u32, iovs_len: u32, nwritten: u32| -> u32 {
            let wasi_iovecs =
                WasiIoVecArrayIter::from(env.memory(), iovs as usize, iovs_len as usize);
            let mut wasi_nwritten = WasiSize::from(env.memory(), nwritten as usize);

            let bytes_written = match fd {
                WASI_STDIN_FILENO => return WASI_EINVAL,
                WASI_STDOUT_FILENO => wasi_iovecs.write(&mut stdout()).unwrap(),
                WASI_STDERR_FILENO => wasi_iovecs.write(&mut stderr()).unwrap(),
                _ => {
                    unimplemented!("Only stdout & stderror allowed for now");
                }
            };

            wasi_nwritten.set(bytes_written as u32);
            WASI_ESUCCESS
        },
    )?;

    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_get",
        move |_: u32, _: u32| -> u32 {
            println!("wasi_snapshot_preview1:fd_prestat_get()");
            8 // WASI_EBADF
        },
    )?;

    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_dir_name",
        move |_: u32, _: u32, _: u32| -> u32 {
            println!("wasi_snapshot_preview1:fd_prestat_dir_name()");
            28 // WASI_EINVAL
        },
    )?;

    linker.func(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        move |_: u32, _: u32| -> u32 {
            println!("wasi_snapshot_preview1:environ_sizes_get()");
            0
        },
    )?;

    linker.func(
        "wasi_snapshot_preview1",
        "environ_get",
        move |_: u32, _: u32| -> u32 {
            println!("wasi_snapshot_preview1:environ_get()");
            0
        },
    )?;

    Ok(())
}
