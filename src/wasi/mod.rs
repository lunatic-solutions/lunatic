mod ptr;
mod types;

use ptr::{Array, WasmPtr};
use types::*;

use crate::process::ProcessEnvironment;
use wasmtime::{Linker, Memory};

use std::cell::Cell;
use std::fmt;
use std::io::{stderr, stdout, Read, Write};

#[derive(Debug, Clone, Copy)]
struct ExitCode(i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExitCode {}

pub fn create_wasi_imports(linker: &mut Linker, process_env_original: ProcessEnvironment) {
    // proc_exit(exit_code)
    let process_env = process_env_original.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "proc_exit",
        move |exit_code: i32| {
            println!("wasi_snapshot_preview1:proc_exit({}) called!", exit_code);
            std::process::exit(exit_code);
        },
    );

    // fd_write(...)
    let process_env = process_env_original.clone();
    linker.func(
        "wasi_snapshot_preview1",
        "fd_write",
        move |fd: i32, iovs: i32, iovs_len: i32, nwritten: i32| -> i32 {
            // let memory = process_env.memory();
            // let iovs_arr_cell = iovs.deref(&memory, 0, iovs_len).unwrap();
            // let nwritten_cell = nwritten.deref(&memory).unwrap();

            // let bytes_written = match fd {
            //     __WASI_STDIN_FILENO => return __WASI_EINVAL,
            //     __WASI_STDOUT_FILENO => {
            //         write_bytes(stdout(), &memory, iovs_arr_cell).unwrap()
            //     }
            //     __WASI_STDERR_FILENO => {
            //         write_bytes(stderr(), &memory, iovs_arr_cell).unwrap()
            //     }
            //     _ => {
            //         unimplemented!("Only stdout & stderror allowed for now");
            //     }
            // };

            // nwritten_cell.set(bytes_written);
            __WASI_ESUCCESS as i32
        },
    );

    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_get",
        move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:fd_prestat_get()");
            8 // WASI_EBADF
        },
    );

    linker.func(
        "wasi_snapshot_preview1",
        "fd_prestat_dir_name",
        move |_: i32, _: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:fd_prestat_dir_name()");
            28 // WASI_EINVAL
        },
    );

    linker.func(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:environ_sizes_get()");
            0
        },
    );

    linker.func(
        "wasi_snapshot_preview1",
        "environ_get",
        move |_: i32, _: i32| -> i32 {
            println!("wasi_snapshot_preview1:environ_get()");
            0
        },
    );
}

// The following functin implementations are taken from wasmer's WASI implementation:
// https://github.com/wasmerio/wasmer/blob/master/lib/wasi/src/syscalls/mod.rs#L48

// fn write_bytes_inner<T: Write>(
//     mut write_loc: T,
//     memory: &Memory,
//     iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
// ) -> Result<u32, __wasi_errno_t> {
//     let mut bytes_written = 0;
//     for iov in iovs_arr_cell {
//         let iov_inner = iov.get();
//         let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
//         write_loc
//             .write_all(&bytes.iter().map(|b_cell| b_cell.get()).collect::<Vec<u8>>())
//             .map_err(|_| __WASI_EIO)?;

//         // TODO: handle failure more accurately
//         bytes_written += iov_inner.buf_len;
//     }
//     Ok(bytes_written)
// }

// fn write_bytes<T: Write>(
//     mut write_loc: T,
//     memory: &Memory,
//     iovs_arr_cell: &[Cell<__wasi_ciovec_t>],
// ) -> Result<u32, __wasi_errno_t> {
//     let result = write_bytes_inner(&mut write_loc, memory, iovs_arr_cell);
//     write_loc.flush().unwrap();
//     result
// }

// fn _read_bytes<T: Read>(
//     mut reader: T,
//     memory: &Memory,
//     iovs_arr_cell: &[Cell<__wasi_iovec_t>],
// ) -> Result<u32, __wasi_errno_t> {
//     let mut bytes_read = 0;

//     for iov in iovs_arr_cell {
//         let iov_inner = iov.get();
//         let bytes = iov_inner.buf.deref(memory, 0, iov_inner.buf_len)?;
//         let raw_bytes: &mut [u8] =
//             unsafe { &mut *(bytes as *const [_] as *mut [_] as *mut [u8]) };
//         bytes_read += reader.read(raw_bytes).map_err(|_| __WASI_EIO)? as u32;
//     }
//     Ok(bytes_read)
// }
