use super::types::*;

use anyhow::Result;
use uptown_funk::{host_functions, types, StateMarker, Trap};
use wasi_common::wasi::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasi_common::{preopen_dir, WasiCtx, WasiCtxBuilder};

use std::{
    fs::File,
    io::{self, IoSlice, IoSliceMut, Read, Write},
    u32,
};

lazy_static::lazy_static! {
    static ref ENV : WasiEnv = WasiEnv::env_vars(std::env::vars());
    static ref ARG : WasiEnv = WasiEnv::args(std::env::args().skip(1));
}

pub struct WasiState {
    ctx: WasiCtx,
}

impl WasiState {
    pub fn new() -> Self {
        let ctx = WasiCtxBuilder::new()
            .inherit_env()
            .inherit_stdio()
            .inherit_args()
            .preopened_dir(preopen_dir("/").unwrap(), "/")
            .build()
            .unwrap();
        Self { ctx }
    }
}

impl StateMarker for WasiState {}

type ExitCode = super::types::ExitCode<WasiState>;
type Ptr<T> = types::Pointer<WasiState, T>;
type Status = Result<types::Status<WasiState>, Trap>;
type Clockid = Wrap<WasiState, wasi_common::wasi::types::Clockid>;
type Fd = Wrap<WasiState, wasi_common::wasi::types::Fd>;

// TODO use correct types for status/error return values

#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    // Command line arguments and environment variables

    fn args_sizes_get(&self, mut var_count: Ptr<u32>, mut total_bytes: Ptr<u32>) -> Status {
        var_count.set(ARG.len());
        total_bytes.set(ARG.total_bytes());
        WasiStatus::Success.into()
    }

    fn args_get(&self, mut args: Ptr<Ptr<u8>>, mut args_buf: Ptr<u8>) -> Status {
        for kv in ARG.iter() {
            args.copy(&args_buf);
            args_buf = args_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the args buffer"))?;
            args = args
                .next()
                .ok_or_else(|| Trap::new("Reached end of the args pointer buffer"))?;
        }

        WasiStatus::Success.into()
    }

    fn environ_sizes_get(&self, mut var_count: Ptr<u32>, mut total_bytes: Ptr<u32>) -> Status {
        var_count.set(ENV.len());
        total_bytes.set(ENV.total_bytes());
        println!("ENV {} = {}", ENV.len(), var_count.value());
        WasiStatus::Success.into()
    }

    fn environ_get(&self, mut environ: Ptr<Ptr<u8>>, mut environ_buf: Ptr<u8>) -> Status {
        for kv in ENV.iter() {
            environ.copy(&environ_buf);
            environ_buf = environ_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the environment variables buffer"))?;
            environ = environ
                .next()
                .ok_or_else(|| Trap::new("Reached end of the environ var pointer buffer"))?;
        }

        WasiStatus::Success.into()
    }

    // Clock, random, yield, exit

    fn clock_res_get(&self, id: Clockid, mut res: Ptr<u64>) -> Error {
        match self.ctx.clock_res_get(id.inner) {
            Ok(c) => {
                res.copy(&c);
                Error::Success
            }
            Err(_) => Error::Inval,
        }
    }

    fn clock_time_get(&self, id: Clockid, precision: u64) -> (Error, u64) {
        match self.ctx.clock_time_get(id.inner, precision) {
            Ok(time) => (Error::Success, time),
            Err(_) => (Error::Inval, 0),
        }
    }

    fn random_get(&self, buf: Ptr<u8>, buf_len: u32) -> u32 {
        // TODO handle error
        getrandom::getrandom(buf.mut_slice(buf_len as usize)).ok();
        WASI_ESUCCESS
    }

    async fn sched_yield(&self) -> u32 {
        smol::future::yield_now().await;
        WASI_ESUCCESS
    }

    fn poll_oneoff(&self, _in: u32, _out: u32, _nsubs: u32, nevents_ptr: u32) -> u32 {
        println!("poll oneoff");
        0
    }

    fn proc_exit(&self, _exit_code: ExitCode) {}
    fn proc_raise(&self, _signal: u32) -> u32 {
        0
    }

    // Filesystem fd functions

    fn fd_advise(&self, _fd: u32, _offset: u64, _len: u64, _advice: u32) -> u32 {
        println!("fd advise");
        0
    }

    fn fd_allocate(&self, _fd: u32, _offset: u64, _len: u64) -> u32 {
        println!("fd allocate");
        0
    }

    fn fd_close(&self, _fd: u32) -> u32 {
        println!("fd close");
        0
    }

    fn fd_datasync(&self, _fd: u32) -> u32 {
        println!("fd datasync");
        0
    }

    fn fd_fdstat_get(&self, _fd: u32, _stat_ptr: u32) -> u32 {
        println!("fd fdstat get");
        0
    }

    fn fd_fdstat_set_flags(&self, _fd: u32, _flags: u32) -> u32 {
        println!("fd fdstat set flags");
        0
    }

    fn fd_fdstat_set_rights(&self, _fd: u32, _rights_base: u64, _rights_inheriting: u64) -> u32 {
        println!("fd fdstat set rigs");
        0
    }

    fn fd_filestat_get(&self, _fd: u32, _filestat_ptr: u32) -> u32 {
        println!("fd filestat get");
        0
    }

    fn fd_filestat_set_size(&self, _fd: u32, _size: u64) -> u32 {
        println!("fd filestat set size");
        0
    }

    fn fd_filestat_set_times(&self, _fd: u32, _atim: u64, _mtim: u64, _fst_flags: u32) -> u32 {
        println!("fd filestat set times");
        0
    }

    fn fd_pread(
        &self,
        _fd: u32,
        _iovs: &mut [IoSliceMut<'_>],
        _offset: u64,
        _nread_ptr: u32,
    ) -> u32 {
        println!("fd pread");
        0
    }

    fn fd_prestat_dir_name(&self, fd: u32, path: Ptr<u8>, path_len: u32) -> u32 {
        println!("fd prestat dir name {} {}", fd, path_len);
        path.copy_slice("/".as_bytes()).ok();
        WASI_ESUCCESS
    }

    fn fd_prestat_get(&self, fd: u32, mut prestat: Ptr<Prestat>) -> u32 {
        println!("fd prestat get {}", fd);
        if fd == 3 {
            prestat.set(Prestat::directory(1));
            return 0;
        } else {
            return WASI_EBADF;
        }
        /*
        let prestat = self.ctx.fd_prestat_get(fd.inner);
        match prestat {
            Ok(prestat) => {
                use wasi_common::wasi::types::Prestat;
                match prestat {
                    Prestat::Dir(d) => (0, dbg!(d.pr_name_len))
                }
            }
            Err(e) => {
                dbg!(e.to_string());
                (WASI_EBADF, 0)
            }
        }
        */
    }

    fn fd_pwrite(&self, _fd: u32, _ciovs: &[IoSlice<'_>], _offset: u64, _nwritten_ptr: u32) -> u32 {
        println!("fd pwrite");
        0
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

    fn fd_readdir(&self, _fd: u32, _buf: &mut [u8], _cookie: u64) -> (u32, u32) {
        println!("fd readdir");
        (0, 0)
    }

    fn fd_renumber(&self, _fd: u32, _to_fd: u32) -> u32 {
        println!("fd renumber");
        0
    }

    fn fd_seek(&self, _fd: u32, _delta: i64, _whence: u32, _newoffset_u64ptr: u32) -> u32 {
        println!("fd seek");
        0
    }

    fn fd_sync(&self, _fd: u32) -> u32 {
        println!("fd sync");
        0
    }

    fn fd_tell(&self, _fd: u32, _offset_u64ptr: u32) -> u32 {
        println!("fd tell");
        0
    }

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

    // Path

    fn path_create_directory(&self, _fd: u32, _path: &str) -> u32 {
        println!("path create dir");
        0
    }

    fn path_filestat_get(&self, fd: Fd, _flags: u32, _path: &str) -> (u32, u32) {
        println!("path filestat get {}", fd.inner);
        (0, 0)
    }

    fn path_filestat_set_times(
        &self,
        _fd: Fd,
        _flags: u32,
        _path: &str,
        _atim: u64,
        _mtim: u64,
        _fst_flags: u32,
    ) -> u32 {
        println!("path path_filestat_set_times");
        0
    }

    fn path_link(
        &self,
        _fd: Fd,
        _old_flags: u32,
        _path: &str,
        _new_fd: Fd,
        _new_path: &str,
    ) -> u32 {
        println!("path link");
        0
    }

    fn path_open(
        &self,
        _fd: Fd,
        _dirflags: u32,
        _path: &str,
        _oflags: u32,
        _fs_rights_base: u64,
        _fs_rights_inheriting: u64,
        _fdflags: u32,
        _opened_fd_ptr: u32,
    ) -> u32 {
        println!("path open {}", _path);
        0
    }

    fn path_readlink(
        &self,
        _fd: Fd,
        _path: &str,
        _buf: u32,
        _buf_len: u32,
        _bufused_ptr: u32,
    ) -> u32 {
        println!("path readlink");
        0
    }

    fn path_remove_directory(&self, _fd: MyFd, _path: &str) -> u32 {
        println!("path remove dir");
        0
    }

    fn path_rename(&self, _fd: Fd, _path: &str, _new_fd: Fd, _new_path: &str) -> u32 {
        println!("path rename");
        0
    }

    fn path_symlink(&self, _old_path: &str, _fd: Fd, _new_path: &str) -> u32 {
        println!("path symlink");
        0
    }

    fn path_unlink_file(&self, _fd: Fd, _path: &str) -> u32 {
        println!("path unlink");
        0
    }

    // Socket

    fn sock_recv(
        &self,
        _fd: Fd,
        _ciovs: &[IoSlice<'_>],
        _ri_flags: u32,
        _ro_datalen_ptr: u32,
        _ro_flags_ptr: u32,
    ) -> u32 {
        println!("sock recv");
        0
    }

    fn sock_send(
        &self,
        _fd: Fd,
        _si_data: &[IoSlice<'_>],
        _si_flags: u32,
        _ro_datalen_ptr: u32,
    ) -> u32 {
        println!("sock send");
        0
    }

    fn sock_shutdown(&self, _fd: Fd, _how: u32) -> u32 {
        println!("sock shutdown");
        0
    }
}
