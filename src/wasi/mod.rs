mod types;
mod ptr;

use types::*;
use ptr::{WasmPtr, Array};

use wasmer::{Store, Function, Exports, ImportObject, Memory, RuntimeError};
use crate::process::creator::ImportEnv;

use std::io::{stdout, stderr, Write, Read};
use std::cell::Cell;
use std::fmt;

#[derive(Debug, Clone, Copy)]
struct ExitCode(i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExitCode {}

pub fn create_wasi_imports(store: Store, resolver: &mut ImportObject, import_env: ImportEnv) {
    let mut wasi_env = Exports::new();

    // proc_exit(exit_code)
    fn proc_exit(exit_code: i32) {
        println!("wasi_snapshot_preview1:proc_exit({}) called!", exit_code);
        RuntimeError::raise(Box::new(ExitCode(exit_code)));
    }
    wasi_env.insert("proc_exit", Function::new_native(&store, proc_exit));

    // fd_write(...)
    fn fd_write(
        env: &mut ImportEnv,
        fd: __wasi_fd_t,
        iovs: WasmPtr<__wasi_ciovec_t, Array>,
        iovs_len: u32,
        nwritten: WasmPtr<u32>,
    ) -> __wasi_errno_t {
        let memory = env.process.borrow().as_ref().unwrap().memory.clone();
        let iovs_arr_cell = iovs.deref(&memory, 0, iovs_len).unwrap();
        let nwritten_cell = nwritten.deref(&memory).unwrap();

        let bytes_written = match fd {
            __WASI_STDIN_FILENO => return __WASI_EINVAL,
            __WASI_STDOUT_FILENO => {
                write_bytes(stdout(), &memory, iovs_arr_cell).unwrap()
            }
            __WASI_STDERR_FILENO => {
                write_bytes(stderr(), &memory, iovs_arr_cell).unwrap()
            }
            _ => {
                unimplemented!("Only stdout & stderror allowed for now");
            }
        };

        nwritten_cell.set(bytes_written);
        __WASI_ESUCCESS
    }
    wasi_env.insert("fd_write", Function::new_native_with_env(&store, import_env, fd_write));


    fn fd_prestat_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:fd_prestat_get()");
        8 // WASI_EBADF
    }
    wasi_env.insert("fd_prestat_get", Function::new_native(&store, fd_prestat_get));

    fn fd_prestat_dir_name(_: i32, _: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:fd_prestat_dir_name()");
        28 // WASI_EINVAL
    }
    wasi_env.insert("fd_prestat_dir_name", Function::new_native(&store, fd_prestat_dir_name));


    fn environ_sizes_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:environ_sizes_get()");
        0
    }
    wasi_env.insert("environ_sizes_get", Function::new_native(&store, environ_sizes_get));

    fn environ_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:environ_get()");
        0
    }
    wasi_env.insert("environ_get", Function::new_native(&store, environ_get));

    resolver.register("wasi_snapshot_preview1", wasi_env);
}


// The following functin implementations are taken from wasmer's WASI implementation:
// https://github.com/wasmerio/wasmer/blob/master/lib/wasi/src/syscalls/mod.rs#L48

fn write_bytes_inner<T: Write>(
    mut write_loc: T,
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
) -> Result<u32, __wasi_errno_t> {
    let mut bytes_written = 0;
    for iov in iovs_arr_cell {
        let iov_inner = iov.get();
        let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
        write_loc
            .write_all(&bytes.iter().map(|b_cell| b_cell.get()).collect::<Vec<u8>>())
            .map_err(|_| __WASI_EIO)?;

        // TODO: handle failure more accurately
        bytes_written += iov_inner.buf_len;
    }
    Ok(bytes_written)
}

fn write_bytes<T: Write>(
    mut write_loc: T,
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
) -> Result<u32, __wasi_errno_t> {
    let result = write_bytes_inner(&mut write_loc, memory, iovs_arr_cell);
    write_loc.flush();
    result
}

fn read_bytes<T: Read>(
    mut reader: T,
    memory: &Memory,
    iovs_arr_cell: &[Cell<__wasi_iovec_t>],
) -> Result<u32, __wasi_errno_t> {
    let mut bytes_read = 0;

    for iov in iovs_arr_cell {
        let iov_inner = iov.get();
        let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
        let mut raw_bytes: &mut [u8] =
            unsafe { &mut *(bytes as *const [_] as *mut [_] as *mut [u8]) };
        bytes_read += reader.read(raw_bytes).map_err(|_| __WASI_EIO)? as u32;
    }
    Ok(bytes_read)
}