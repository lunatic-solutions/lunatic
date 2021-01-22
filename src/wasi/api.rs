use super::types::*;

use uptown_funk::{host_functions, types, Executor, FromWasm, StateMarker, ToWasm, Trap};
use wasi_common::wasi::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasi_common::{WasiCtx, WasiCtxBuilder};

use std::{
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
        let ctx = WasiCtxBuilder::new().build().unwrap();
        Self { ctx }
    }
}

struct Fd {
    inner: u32,
}

impl FromWasm for Fd {
    type From = u32;
    type State = WasiState;

    fn from(_: &mut WasiState, _: &impl Executor, v: u32) -> Result<Fd, Trap> {
        Ok(Fd { inner: v })
    }
}

impl ToWasm for Fd {
    type To = u32;
    type State = WasiState;

    fn to(_: &mut WasiState, _: &impl Executor, fd: Fd) -> Result<u32, Trap> {
        Ok(fd.inner)
    }
}

impl StateMarker for WasiState {}

type Ptr<T> = types::Pointer<T>;
type Clockid = Wrap<wasi_common::wasi::types::Clockid>;
type StatusOrTrap = Result<Status, Trap>;

#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    // Command line arguments and environment variables

    fn args_sizes_get(&self, mut var_count: Ptr<Size>, mut total_bytes: Ptr<Size>) -> Status {
        var_count.set(ARG.len());
        total_bytes.set(ARG.total_bytes());
        Status::Success
    }

    fn args_get(&self, mut args: Ptr<Ptr<u8>>, mut args_buf: Ptr<u8>) -> StatusOrTrap {
        for kv in ARG.iter() {
            args.copy(&args_buf);
            args_buf = args_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the args buffer"))?;
            args = args
                .next()
                .ok_or_else(|| Trap::new("Reached end of the args pointer buffer"))?;
        }

        Ok(Status::Success)
    }

    fn environ_sizes_get(&self, mut var_count: Ptr<Size>, mut total_bytes: Ptr<Size>) -> Status {
        var_count.set(ENV.len());
        total_bytes.set(ENV.total_bytes());
        println!("ENV {} = {}", ENV.len(), var_count.value());
        Status::Success
    }

    fn environ_get(&self, mut environ: Ptr<Ptr<u8>>, mut environ_buf: Ptr<u8>) -> StatusOrTrap {
        for kv in ENV.iter() {
            environ.copy(&environ_buf);
            environ_buf = environ_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the environment variables buffer"))?;
            environ = environ
                .next()
                .ok_or_else(|| Trap::new("Reached end of the environ var pointer buffer"))?;
        }

        Ok(Status::Success)
    }

    // Clock, random, yield, exit

    fn clock_res_get(&self, id: Clockid, mut res: Ptr<Timestamp>) -> Status {
        match self.ctx.clock_res_get(id.inner) {
            Ok(c) => {
                res.copy(&c);
                Status::Success
            }
            Err(_) => Status::Inval,
        }
    }

    fn clock_time_get(&self, id: Clockid, precision: Timestamp) -> (Status, Timestamp) {
        match self.ctx.clock_time_get(id.inner, precision) {
            Ok(time) => (Status::Success, time),
            Err(_) => (Status::Inval, 0),
        }
    }

    fn random_get(&self, buf: Ptr<u8>, buf_len: Size) -> StatusOrTrap {
        getrandom::getrandom(buf.mut_slice(buf_len as usize))
            .map_err(|_| Trap::new("Error getting random bytes"))?;
        Ok(Status::Success)
    }

    async fn sched_yield(&self) -> Status {
        smol::future::yield_now().await;
        Status::Success
    }

    fn poll_oneoff(&self, _in: u32, _out: u32, _nsubs: u32, _nevents_ptr: u32) -> Status {
        println!("poll oneoff");
        Status::Success
    }

    fn proc_exit(&self, _exit_code: ExitCode) {}
    fn proc_raise(&self, _signal: Signal) -> Status {
        Status::Success
    }

    // Filesystem fd functions

    fn fd_advise(&self, _fd: u32, _offset: u64, _len: u64, _advice: u32) -> Status {
        // Ignore advise
        Status::Success
    }

    fn fd_allocate(&self, _fd: u32, _offset: u64, _len: u64) -> Status {
        println!("fd allocate");
        Status::Success
    }

    fn fd_close(&self, _fd: u32) -> Status {
        println!("fd close");
        Status::Success
    }

    fn fd_datasync(&self, _fd: u32) -> Status {
        println!("fd datasync");
        Status::Success
    }

    fn fd_fdstat_get(&self, _fd: u32, _stat_ptr: u32) -> Status {
        println!("fd fdstat get");
        Status::Success
    }

    fn fd_fdstat_set_flags(&self, _fd: u32, _flags: u32) -> Status {
        println!("fd fdstat set flags");
        Status::Success
    }

    fn fd_fdstat_set_rights(&self, _fd: u32, _rights_base: u64, _rights_inheriting: u64) -> Status {
        println!("fd fdstat set rigs");
        Status::Success
    }

    fn fd_filestat_get(&self, _fd: u32, _filestat_ptr: u32) -> Status {
        println!("fd filestat get");
        Status::Success
    }

    fn fd_filestat_set_size(&self, _fd: u32, _size: u64) -> Status {
        println!("fd filestat set size");
        Status::Success
    }

    fn fd_filestat_set_times(&self, _fd: u32, _atim: u64, _mtim: u64, _fst_flags: u32) -> Status {
        println!("fd filestat set times");
        Status::Success
    }

    fn fd_pread(
        &self,
        _fd: u32,
        _iovs: &mut [IoSliceMut<'_>],
        _offset: u64,
        _nread_ptr: u32,
    ) -> Status {
        println!("fd pread");
        Status::Success
    }

    fn fd_prestat_dir_name(&self, fd: u32, path: Ptr<u8>, path_len: u32) -> Status {
        println!("fd prestat dir name {} {}", fd, path_len);
        path.copy_slice("/".as_bytes()).ok();
        Status::Success
    }

    fn fd_prestat_get(&self, fd: u32, mut prestat: Ptr<Prestat>) -> Status {
        println!("fd prestat get {}", fd);
        if fd == 3 {
            prestat.set(Prestat::directory(1));
            return Status::Success;
        } else {
            return Status::Badf;
        }
    }

    fn fd_pwrite(
        &self,
        _fd: u32,
        _ciovs: &[IoSlice<'_>],
        _offset: u64,
        _nwritten_ptr: u32,
    ) -> Status {
        println!("fd pwrite");
        Status::Success
    }

    fn fd_read(&self, fd: u32, iovs: &mut [IoSliceMut<'_>]) -> (Status, u32) {
        match fd {
            // Stdout & stderr not supported as read destination
            1 | 2 => (Status::Inval, 0),
            0 => {
                let written = io::stdin().read_vectored(iovs).unwrap();
                (Status::Success, written as u32)
            }
            _ => panic!("Unsupported wasi read destination"),
        }
    }

    fn fd_readdir(&self, _fd: u32, _buf: &mut [u8], _cookie: u64) -> (Status, u32) {
        println!("fd readdir");
        (Status::Success, 0)
    }

    fn fd_renumber(&self, _fd: u32, _to_fd: u32) -> Status {
        println!("fd renumber");
        Status::Success
    }

    fn fd_seek(&self, _fd: u32, _delta: i64, _whence: u32, _newoffset_u64ptr: u32) -> Status {
        println!("fd seek");
        Status::Success
    }

    fn fd_sync(&self, _fd: u32) -> Status {
        println!("fd sync");
        Status::Success
    }

    fn fd_tell(&self, _fd: u32, _offset_u64ptr: u32) -> Status {
        println!("fd tell");
        Status::Success
    }

    fn fd_write(&self, fd: u32, ciovs: &[IoSlice<'_>]) -> (Status, u32) {
        match fd {
            // Stdin not supported as write destination
            0 => (Status::Inval, 0),
            1 => {
                let written = io::stdout().write_vectored(ciovs).unwrap();
                (Status::Success, written as u32)
            }
            2 => {
                let written = io::stderr().write_vectored(ciovs).unwrap();
                (Status::Success, written as u32)
            }
            _ => panic!("Unsupported wasi write destination"),
        }
    }

    // Path

    fn path_create_directory(&self, _fd: u32, _path: &str) -> Status {
        println!("path create dir");
        Status::Success
    }

    fn path_filestat_get(&self, fd: u32, _flags: u32, _path: &str) -> (Status, u32) {
        println!("path filestat get {}", fd);
        (Status::Success, 0)
    }

    fn path_filestat_set_times(
        &self,
        _fd: Fd,
        _flags: u32,
        _path: &str,
        _atim: u64,
        _mtim: u64,
        _fst_flags: u32,
    ) -> Status {
        println!("path path_filestat_set_times");
        Status::Success
    }

    fn path_link(
        &self,
        _fd: Fd,
        _old_flags: u32,
        _path: &str,
        _new_fd: Fd,
        _new_path: &str,
    ) -> Status {
        println!("path link");
        Status::Success
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
    ) -> Status {
        println!("path open {}", _path);
        Status::Success
    }

    fn path_readlink(
        &self,
        _fd: Fd,
        _path: &str,
        _buf: u32,
        _buf_len: u32,
        _bufused_ptr: u32,
    ) -> Status {
        println!("path readlink");
        Status::Success
    }

    fn path_remove_directory(&self, _fd: u32, _path: &str) -> Status {
        println!("path remove dir");
        Status::Success
    }

    fn path_rename(&self, _fd: Fd, _path: &str, _new_fd: Fd, _new_path: &str) -> Status {
        println!("path rename");
        Status::Success
    }

    fn path_symlink(&self, _old_path: &str, _fd: Fd, _new_path: &str) -> Status {
        println!("path symlink");
        Status::Success
    }

    fn path_unlink_file(&self, _fd: Fd, _path: &str) -> Status {
        println!("path unlink");
        Status::Success
    }

    // Socket

    fn sock_recv(
        &self,
        _fd: Fd,
        _ciovs: &[IoSlice<'_>],
        _ri_flags: u32,
        _ro_datalen_ptr: u32,
        _ro_flags_ptr: u32,
    ) -> Status {
        println!("sock recv");
        Status::Success
    }

    fn sock_send(
        &self,
        _fd: Fd,
        _si_data: &[IoSlice<'_>],
        _si_flags: u32,
        _ro_datalen_ptr: u32,
    ) -> Status {
        println!("sock send");
        Status::Success
    }

    fn sock_shutdown(&self, _fd: Fd, _how: u32) -> Status {
        println!("sock shutdown");
        Status::Inval
    }
}
