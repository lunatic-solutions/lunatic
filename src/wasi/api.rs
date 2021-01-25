use super::types::*;

use uptown_funk::{host_functions, types, Trap};
use wasi_common::wasi::wasi_snapshot_preview1::WasiSnapshotPreview1;

use std::io::{self, IoSlice, IoSliceMut, Read, SeekFrom, Write};

lazy_static::lazy_static! {
    static ref ENV : WasiEnv = WasiEnv::env_vars(std::env::vars());
    static ref ARG : WasiEnv = WasiEnv::args(std::env::args().skip(1));
}

pub use super::state::WasiState;

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

    fn fd_allocate(&self, _fd: Fd, _offset: Filesize, _len: Filesize) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_close(&mut self, fd: Fd) -> Status {
        self.close(fd);
        Status::Success
    }

    fn fd_datasync(&self, _fd: Fd) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_fdstat_get(&self, fd: Fd, mut stat: Ptr<Fdstat>) -> Status {
        let metadata = self.filestat(fd);
        stat.set(Fdstat {
            fs_filetype: metadata.unwrap().filetype,
            fs_flags: Fdflags::new(),
            fs_rights_base: 0x600,
            fs_rights_inheriting: 0x600,
        });
        Status::Success
    }

    fn fd_fdstat_set_flags(&self, _fd: u32, _flags: u32) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_fdstat_set_rights(&self, _fd: u32, _rights_base: u64, _rights_inheriting: u64) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_filestat_get(&self, fd: Fd, mut filestat: Ptr<Filestat>) -> Status {
        if let Some(fstat) = self.filestat(fd) {
            dbg!(fstat);
            filestat.set(fstat);
            Status::Success
        } else {
            Status::Badf
        }
    }

    fn fd_filestat_set_size(&mut self, fd: Fd, size: Filesize) -> Status {
        self.set_size(fd, size);
        Status::Success
    }

    fn fd_filestat_set_times(&self, _fd: u32, _atim: u64, _mtim: u64, _fst_flags: u32) -> Status {
        println!("fd filestat set times");
        // Ignore for now
        Status::Success
    }

    fn fd_pread(
        &mut self,
        fd: u32,
        iovs: &mut [IoSliceMut<'_>],
        offset: Filesize,
    ) -> (Status, u32) {
        let tell = self.tell(fd).unwrap();
        self.seek(fd, SeekFrom::Start(offset));
        let ret = self.fd_read(fd, iovs);
        self.seek(fd, SeekFrom::Start(tell));
        ret
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

    fn fd_pwrite(&mut self, fd: u32, ciovs: &[IoSlice<'_>], offset: Filesize) -> (Status, u32) {
        let tell = self.tell(fd).unwrap();
        self.seek(fd, SeekFrom::Start(offset));
        let ret = self.fd_write(fd, ciovs);
        self.seek(fd, SeekFrom::Start(tell));
        ret
    }

    fn fd_read(&mut self, fd: u32, iovs: &mut [IoSliceMut<'_>]) -> (Status, u32) {
        match fd {
            // Stdout & stderr not supported as read destination
            1 | 2 => (Status::Inval, 0),
            0 => {
                let written = io::stdin().read_vectored(iovs).unwrap();
                (Status::Success, written as u32)
            }
            fd => {
                let written = self.read(fd, iovs).unwrap();
                (Status::Success, written as u32)
            }
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

    fn fd_seek(&mut self, fd: Fd, delta: Filedelta, whence: Whence) -> (Status, u64) {
        (
            Status::Success,
            self.seek(fd, whence.into_seek_from(delta)).unwrap_or(0),
        )
    }

    fn fd_sync(&self, _fd: u32) -> Status {
        println!("fd sync");
        // Ignore for now
        Status::Success
    }

    fn fd_tell(&mut self, fd: Fd) -> (Status, u64) {
        (Status::Success, self.tell(fd).unwrap_or(0))
    }

    fn fd_write(&mut self, fd: Fd, ciovs: &[IoSlice<'_>]) -> (Status, u32) {
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
            fd => {
                let written = self.write(fd, ciovs).unwrap();
                (Status::Success, written as u32)
            }
        }
    }

    // Path

    fn path_create_directory(&self, fd: Fd, path: &str) -> Status {
        let abs_path = self.abs_path(fd, path).unwrap();
        self.create_directory(abs_path)
    }

    fn path_filestat_get(
        &self,
        fd: Fd,
        _flags: u32,
        path: &str,
        mut filestat: Ptr<Filestat>,
    ) -> Status {
        if let Some(abs_path) = self.abs_path(fd, path) {
            if let Some(fstat) = self.filestat_path(&abs_path) {
                dbg!(fstat);
                filestat.set(fstat);
                Status::Success
            } else {
                Status::Badf
            }
        } else {
            Status::Badf
        }
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
        // Ignore for now
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
        // Ignore for now
        Status::Success
    }

    /// Open a file or directory.
    fn path_open(
        &mut self,
        fd: Fd,
        dirflags: Lookupflags,
        path: &str,
        oflags: OpenFlags,
        _fs_rights_base: Rights,
        _fs_rights_inheriting: Rights,
        fdflags: Fdflags,
    ) -> (Status, Fd) {
        println!(
            "path open {} from {:?}: {:?} {:?} {:?}",
            path, fd, dirflags, oflags, fdflags
        );
        let abs_path = self.abs_path(fd, path).unwrap();
        let fd = self.open(abs_path, oflags);
        (Status::Success, fd)
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
        // Ignore for now
        Status::Success
    }

    fn path_remove_directory(&self, fd: Fd, path: &str) -> Status {
        let abs_path = self.abs_path(fd, path).unwrap();
        self.remove_directory(abs_path)
    }

    fn path_rename(&self, fd: Fd, path: &str, new_fd: Fd, new_path: &str) -> Status {
        let from = self.abs_path(fd, path).unwrap();
        let to = self.abs_path(new_fd, new_path).unwrap();
        self.rename(from, to)
    }

    fn path_symlink(&self, _old_path: &str, _fd: Fd, _new_path: &str) -> Status {
        println!("path symlink");
        // Ignore for now
        Status::Success
    }

    fn path_unlink_file(&self, _fd: Fd, _path: &str) -> Status {
        println!("path unlink");
        // Ignore for now
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
