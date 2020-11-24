use super::types::*;

use crate::process::ProcessEnvironment;

use anyhow::Result;
use smol::net::TcpStream;
use smol::prelude::{AsyncReadExt, AsyncWriteExt};
use wasmtime::{ExternRef, Func, FuncType, Linker, Trap, ValType::*};

use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Stderr, Stdin, Stdout, Write};

#[derive(Debug, Clone, Copy)]
struct ExitCode(i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExitCode {}

pub fn add_to_linker(linker: &mut Linker, environment: &ProcessEnvironment) -> Result<()> {
    // proc_exit(exit_code)
    linker.func(
        "wasi_snapshot_preview1",
        "proc_exit",
        move |_exit_code: i32| -> Result<(), Trap> { Err(Trap::new("proc_exit() called")) },
    )?;

    // fd_write(...)
    let env = environment.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "fd_write",
        move |fd: Option<ExternRef>, ciovs: u32, ciovs_len: u32, nwritten_ptr: u32| -> u32 {
            let wasi_ciovecs =
                WasiConstIoVecArray::from(env.memory(), ciovs as usize, ciovs_len as usize);
            let mut wasi_nwritten = WasiSize::from(env.memory(), nwritten_ptr as usize);

            let fd = fd.unwrap();
            let fd = fd.data();
            if let Some(_) = fd.downcast_ref::<Stdin>() {
                return WASI_EINVAL;
            }

            let bytes_written = if let Some(mut stdout) = fd.downcast_ref::<Stdout>() {
                stdout.write_vectored(wasi_ciovecs.get_io_slices()).unwrap()
            } else if let Some(mut stderr) = fd.downcast_ref::<Stderr>() {
                stderr.write_vectored(wasi_ciovecs.get_io_slices()).unwrap()
            } else if let Some(tcp_stream) = fd.downcast_ref::<RefCell<TcpStream>>() {
                env.async_(
                    tcp_stream
                        .borrow_mut()
                        .write_vectored(wasi_ciovecs.get_io_slices()),
                )
                .unwrap()
            } else {
                unimplemented!();
            };

            wasi_nwritten.set(bytes_written as u32);
            WASI_ESUCCESS
        },
    )?;

    // fd_read(...)
    let env = environment.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "fd_read",
        move |fd: Option<ExternRef>, iovs: u32, iovs_len: u32, nread_ptr: u32| -> u32 {
            let mut wasi_iovecs =
                WasiIoVecArray::from(env.memory(), iovs as usize, iovs_len as usize);
            let mut wasi_nread = WasiSize::from(env.memory(), nread_ptr as usize);

            let fd = fd.unwrap();
            let fd = fd.data();
            if fd.downcast_ref::<Stdout>().is_some() || fd.downcast_ref::<Stderr>().is_some() {
                return WASI_EINVAL;
            }

            let bytes_read = if let Some(stdin) = fd.downcast_ref::<Stdin>() {
                stdin
                    .lock()
                    .read_vectored(wasi_iovecs.get_io_slices_mut())
                    .unwrap()
            } else if let Some(tcp_stream) = fd.downcast_ref::<RefCell<TcpStream>>() {
                let mut tcp_stream = tcp_stream.borrow_mut();
                env.async_(tcp_stream.read_vectored(wasi_iovecs.get_io_slices_mut()))
                    .unwrap()
            } else {
                unimplemented!();
            };

            wasi_nread.set(bytes_read as u32);
            WASI_ESUCCESS
        },
    )?;

    // path_open(...)
    let _env = environment.clone();
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

    let env = environment.clone();
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


    // Create a vector of byte vectors representing environment variables.
    // The vector will be copied to the guest memory on `environ_get`.
    let mut env_bytes: Vec<Vec<u8>> = vec![]; 
    for (k, v) in std::env::vars() {
        env_bytes.push(format!("{}={}\0", k, v).into_bytes());
    }
    let env_bytes_copy = env_bytes.clone();

    let env = environment.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        move |environc: u32, environ_buf_size: u32| -> u32 {
            println!("wasi_snapshot_preview1:environ_sizes_get({}, {})", environc, environ_buf_size);
            // TODO needs better abstraction. This is manual memory poking, very error prone
            unsafe { 
                let number_of_elements = env.memory().add(environc as usize) as *mut u32;
                *number_of_elements = (&env_bytes).len() as u32;

                let total_size = env.memory().add(environ_buf_size as usize) as *mut u32;
                *total_size = (&env_bytes).iter().map(|v| v.len()as u32).sum();
            }
            0
        },
    )?;

    let env = environment.clone();
    let env_bytes = env_bytes_copy;
    linker.func(
        "wasi_snapshot_preview1",
        "environ_get",
        move |environ: u32, environ_buf: u32| -> u32 {
            println!("wasi_snapshot_preview1:environ_get({}, {})", environ, environ_buf);
            // TODO needs better abstraction. This is manual memory poking, very error prone
            unsafe { 
                let mut size = 0;
                for (i, entry) in env_bytes.iter().enumerate() {
                    let ith = env.memory().add(environ as usize + 4 * i as usize) as *mut u32;
                    *ith = environ_buf + size;
                    for (j, byte) in entry.iter().enumerate() {
                        let p = env.memory().add(environ_buf as usize + size as usize + j) as *mut u8;
                        *p = *byte;
                    }
                    size += entry.len() as u32;
                }
            }
            0
        },
    )?;

    Ok(())
}
