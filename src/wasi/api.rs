use super::types::*;

use crate::process::ProcessEnvironment;

use anyhow::Result;
use wasmtime::{ExternRef, Func, FuncType, Linker, Trap, ValType::*};

use std::fmt;
use std::io::{Stderr, Stdin, Stdout};

#[derive(Debug, Clone, Copy)]
struct ExitCode(i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExitCode {}

pub fn add_to_linker(linker: &mut Linker, process_env_original: &ProcessEnvironment) -> Result<()> {
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
        move |fd: Option<ExternRef>, iovs: u32, iovs_len: u32, nwritten: u32| -> u32 {
            let wasi_iovecs =
                WasiIoVecArrayIter::from(env.memory(), iovs as usize, iovs_len as usize);
            let mut wasi_nwritten = WasiSize::from(env.memory(), nwritten as usize);

            let fd = fd.unwrap();
            let fd = fd.data();
            if let Some(_) = fd.downcast_ref::<Stdin>() {
                return WASI_EINVAL;
            }

            let bytes_written = if let Some(mut stdout) = fd.downcast_ref::<Stdout>() {
                wasi_iovecs.write(&mut stdout).unwrap()
            } else if let Some(mut stderr) = fd.downcast_ref::<Stderr>() {
                wasi_iovecs.write(&mut stderr).unwrap()
            } else {
                unimplemented!();
            };

            wasi_nwritten.set(bytes_written as u32);
            WASI_ESUCCESS
        },
    )?;

    // path_open(...)
    let _env = process_env_original.clone();
    let path_open = Func::new(
        linker.store(),
        FuncType::new(
            vec![ExternRef, I32, I32, I32, I32, I64, I64, I32],
            vec![I32, ExternRef],
        ),
        |_caller, _params, _result| -> Result<(), Trap> {
            // println!("wasi_snapshot_preview1:path_open({})", fd);
            // let path = WasiString::from(env.memory(), path_ptr as usize, path_len as usize);
            // let file = env.async_(fs::File::create(path.get())).unwrap();
            // let fd_value = RESOURCES.add(Resource::Owned(ResourceTypeOwned::File(file))) as u32;
            // let mut fd = WasiSize::from(env.memory(), opened_fd_ptr as usize);
            // fd.set(fd_value + FD_OFFSET);
            // WASI_ESUCCESS

            Ok(())
        },
    );
    linker.define("wasi_snapshot_preview1", "path_open", path_open)?;

    linker.func(
        "wasi_snapshot_preview1",
        "fd_close",
        move |fd: u32| -> u32 {
            println!("wasi_snapshot_preview1:fd_close({})", fd);
            WASI_ESUCCESS
        },
    )?;

    let env = process_env_original.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_get",
        move |fd: u32, prestat_ptr: u32| -> u32 {
            println!("wasi_snapshot_preview1:fd_prestat_get({})", fd);
            // ALlow access to all directories
            if fd == 3 {
                let mut prestat = WasiPrestatDir::from(env.memory(), prestat_ptr as usize);
                prestat.set_dir_len(0);
                WASI_ESUCCESS
            } else {
                WASI_EBADF
            }
        },
    )?;

    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_dir_name",
        move |fd: u32, _path_ptr: u32, path_len: u32| -> u32 {
            println!("wasi_snapshot_preview1:fd_prestat_dir_name()");
            if fd == 3 {
                if path_len != 0 {
                    panic!("path len can only be the value passed in fd_prestat_get");
                }
                WASI_ESUCCESS
            } else {
                WASI_EINVAL
            }
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
            WASI_ENOTSUP
        },
    )?;

    Ok(())
}
