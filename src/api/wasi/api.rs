use super::types::*;

use log::debug;
use uptown_funk::{host_functions, types, Trap};

use std::io::{self, IoSlice, IoSliceMut, Read, SeekFrom, Write};

lazy_static::lazy_static! {
    static ref ENV : WasiEnv = WasiEnv::env_vars(std::env::vars());
    static ref ARG : WasiEnv = WasiEnv::args(std::env::args().skip(1));
}

pub use super::state::WasiState;

type Ptr<T> = types::Pointer<T>;

#[cfg(any(
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android",
    target_os = "macos"
))]
use super::unix::*;

#[cfg(any(target_os = "windows"))]
use super::windows::*;

#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    // Command line arguments and environment variables

    fn args_sizes_get(&self, mut var_count: Ptr<Size>, mut total_bytes: Ptr<Size>) -> Status {
        var_count.set(ARG.len());
        total_bytes.set(ARG.total_bytes());
        Status::Success
    }

    fn args_get(&self, mut args: Ptr<Ptr<u8>>, mut args_buf: Ptr<u8>) -> StatusTrapResult {
        for kv in ARG.iter() {
            args.copy(&args_buf);
            args_buf = args_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the args buffer"))?;
            args = args
                .next()
                .ok_or_else(|| Trap::new("Reached end of the args pointer buffer"))?;
        }

        Ok(())
    }

    fn environ_sizes_get(
        &self,
        mut var_count: Ptr<Size>,
        mut total_bytes: Ptr<Size>,
    ) -> StatusTrapResult {
        var_count.set(ENV.len());
        total_bytes.set(ENV.total_bytes());
        Ok(())
    }

    fn environ_get(&self, mut environ: Ptr<Ptr<u8>>, mut environ_buf: Ptr<u8>) -> StatusTrapResult {
        for kv in ENV.iter() {
            environ.copy(&environ_buf);
            environ_buf = environ_buf
                .copy_slice(&kv)?
                .ok_or_else(|| Trap::new("Reached end of the environment variables buffer"))?;
            environ = environ
                .next()
                .ok_or_else(|| Trap::new("Reached end of the environ var pointer buffer"))?;
        }

        Ok(())
    }

    // Clock, random, yield, exit

    fn clock_res_get(&self, id: Clockid, res: Ptr<Timestamp>) -> Status {
        platform_clock_res_get(id, res)
    }

    fn clock_time_get(
        &self,
        id: Clockid,
        precision: Timestamp,
        time: Ptr<Timestamp>,
    ) -> StatusTrapResult {
        platform_clock_time_get(id, precision, time)
    }

    fn random_get(&self, buf: Ptr<u8>, buf_len: Size) -> StatusTrapResult {
        getrandom::getrandom(buf.mut_slice(buf_len as usize))
            .map_err(|_| Trap::new("Error getting random bytes"))?;
        Ok(())
    }

    async fn sched_yield(&self) -> Status {
        smol::future::yield_now().await;
        Status::Success
    }

    fn poll_oneoff(&self, _in: u32, _out: u32, _nsubs: u32, _nevents_ptr: u32) -> Status {
        // Ignore for now
        Status::Success
    }

    fn proc_exit(&self, exit_code: u32) -> Trap {
        Trap::new(format!("proc_exit({}) called", exit_code))
    }

    fn proc_raise(&self, _signal: Signal) -> Status {
        Status::Success
    }

    // Filesystem fd functions

    fn fd_advise(&self, _fd: u32, _offset: u64, _len: u64, _advice: u32) -> Status {
        // Ignore for now
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

    fn fd_fdstat_get(&self, fd: Fd, mut stat: Ptr<Fdstat>) -> StatusResult {
        let metadata = self.filestat(fd);
        stat.set(Fdstat {
            fs_filetype: metadata?.filetype,
            fs_flags: Fdflags::new(),
            fs_rights_base: 0x600,
            fs_rights_inheriting: 0x600,
        });
        Ok(())
    }

    fn fd_fdstat_set_flags(&self, _fd: u32, _flags: u32) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_fdstat_set_rights(&self, _fd: u32, _rights_base: u64, _rights_inheriting: u64) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_filestat_get(&self, fd: Fd, mut filestat: Ptr<Filestat>) -> StatusResult {
        Ok(filestat.set(self.filestat(fd)?))
    }

    fn fd_filestat_set_size(&mut self, fd: Fd, size: Filesize) -> StatusResult {
        self.set_size(fd, size)
    }

    fn fd_filestat_set_times(&self, _fd: u32, _atim: u64, _mtim: u64, _fst_flags: u32) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_pread(
        &mut self,
        fd: u32,
        iovs: &mut [IoSliceMut<'_>],
        offset: Filesize,
        read_len: Ptr<Size>,
    ) -> StatusResult {
        let tell = self.tell(fd)?;
        self.seek(fd, SeekFrom::Start(offset))?;
        self.fd_read(fd, iovs, read_len)?;
        self.seek(fd, SeekFrom::Start(tell))?;
        Ok(())
    }

    fn fd_prestat_dir_name(&self, _fd: u32, path: Ptr<u8>, _path_len: u32) -> Status {
        path.copy_slice("/".as_bytes()).ok();
        Status::Success
    }

    fn fd_prestat_get(&self, fd: u32, mut prestat: Ptr<Prestat>) -> Status {
        if fd == 3 {
            prestat.set(Prestat::directory(1));
            return Status::Success;
        } else {
            return Status::Badf;
        }
    }

    fn fd_pwrite(
        &mut self,
        fd: u32,
        ciovs: &[IoSlice<'_>],
        offset: Filesize,
        write_len: Ptr<Size>,
    ) -> StatusResult {
        debug!("fd_pwrite fd={}, offset={}", fd, offset);
        let tell = self.tell(fd)?;
        self.seek(fd, SeekFrom::Start(offset))?;
        self.fd_write(fd, ciovs, write_len)?;
        self.seek(fd, SeekFrom::Start(tell))?;
        Ok(())
    }

    fn fd_read(
        &mut self,
        fd: u32,
        iovs: &mut [IoSliceMut<'_>],
        mut read_len: Ptr<Size>,
    ) -> StatusResult {
        debug!("fd_read fd={}", fd);
        let read = match fd {
            // Stdout & stderr not supported as read destination
            1 | 2 => return Status::Inval.into(),
            0 => io::stdin().read_vectored(iovs)?,
            fd => self.read(fd, iovs)?,
        };
        read_len.set(read as u32);
        Ok(())
    }

    #[allow(unused_assignments)]
    fn fd_readdir(
        &self,
        fd: Fd,
        mut buf: Ptr<u8>,
        buf_len: Size,
        cookie: Dircookie,
        mut written_ptr: Ptr<Size>,
    ) -> StatusTrapResult {
        debug!(
            "fd_readdir fd={}, buf={:?}, buf_len={}, cookie={}",
            fd, buf, buf_len, cookie
        );
        let path = self.get_path(fd)?;
        let paths = std::fs::read_dir(path)?;
        let dirent_size = std::mem::size_of::<Dirent>() as u32;
        let mut written = 0;
        // TODO something is broken when multiple entries are sent
        // check how Rust implements reading from this syscall
        for (i, dent) in paths
            .filter(|e| e.is_ok())
            .map(|e| e.unwrap())
            .enumerate()
            .skip(cookie as usize)
        {
            let dpath = dent.file_name();
            let dpath_str = dpath.to_str();
            if dpath_str.is_none() {
                continue;
            }
            let dpath_bytes = dpath_str.unwrap().as_bytes();
            if written + dpath_bytes.len() as u32 + dirent_size > buf_len {
                break;
            }

            let d = Dirent {
                d_next: (i + 1) as u64,
                d_ino: 0,
                d_namlen: dpath_bytes.len() as u32,
                d_type: dent.file_type()?.into(),
            };
            //debug!("Write {} direntzie to {:?}", dirent_size, buf); // WTF
            buf = buf
                .set_cast(d)?
                .ok_or_else(|| Trap::new("Reached end of the readdir buffer"))?;
            //debug!("Copy {} bytes to {:?}", dpath_bytes.len(), buf);

            buf = buf
                .copy_slice(&dpath_bytes)?
                .ok_or_else(|| Trap::new("Reached end of the readdir buffer"))?;
            //debug!("Final buf {:?}", buf);
            written += dirent_size + dpath_bytes.len() as u32;
            //debug!("Name: {} {:?}", i, dent.path());
            //debug!("Written {}", written);
            break;
        }

        written_ptr.set(written);
        Ok(())
    }

    fn fd_renumber(&self, _fd: u32, _to_fd: u32) -> Status {
        Status::Success
    }

    fn fd_seek(
        &mut self,
        fd: Fd,
        delta: Filedelta,
        whence: Whence,
        mut seek_res: Ptr<u64>,
    ) -> StatusResult {
        Ok(seek_res.set(self.seek(fd, whence.into_seek_from(delta))?))
    }

    fn fd_sync(&self, _fd: u32) -> Status {
        // Ignore for now
        Status::Success
    }

    fn fd_tell(&mut self, fd: Fd, mut tell_res: Ptr<u64>) -> StatusResult {
        Ok(tell_res.set(self.tell(fd)?))
    }

    fn fd_write(
        &mut self,
        fd: Fd,
        ciovs: &[IoSlice<'_>],
        mut write_len: Ptr<Size>,
    ) -> StatusResult {
        let written = match fd {
            // Stdin not supported as write destination
            0 => return Status::Inval.into(),
            1 => io::stdout().write_vectored(ciovs)?,
            2 => io::stderr().write_vectored(ciovs)?,
            fd => self.write(fd, ciovs)?,
        };
        write_len.set(written as u32);
        Ok(())
    }

    // Path

    fn path_create_directory(&self, fd: Fd, path: &str) -> StatusResult {
        debug!("path_create_directory fd={}, path={}", fd, path);
        let abs_path = self.abs_path(fd, path)?;
        self.create_directory(abs_path)
    }

    fn path_filestat_get(
        &self,
        fd: Fd,
        flags: u32,
        path: &str,
        mut filestat: Ptr<Filestat>,
    ) -> StatusResult {
        debug!(
            "path_filestat_get fd={}, offset={:X}, path={}",
            fd, flags, path
        );

        let abs_path = self.abs_path(fd, path)?;
        filestat.set(self.filestat_path(&abs_path)?);

        Ok(())
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
        // Ignore for now
        Status::Success
    }

    fn path_link(
        &self,
        fd: Fd,
        _old_flags: u32,
        path: &str,
        new_fd: Fd,
        new_path: &str,
    ) -> StatusResult {
        // TODO handle flags
        let old = self.abs_path(fd, path)?;
        let new = self.abs_path(new_fd, new_path)?;
        std::fs::hard_link(old, new)?;
        Status::Success.into()
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
        _fdflags: Fdflags,
        mut fd_res: Ptr<Fd>,
    ) -> StatusResult {
        debug!(
            "path_open fd={}, dirflags={:X}, oflags={:X}, path={}",
            fd,
            dirflags,
            oflags.inner(),
            path
        );
        let abs_path = self.abs_path(fd, path)?;
        let fd = self.open(abs_path, oflags)?;
        fd_res.set(fd);
        Ok(())
    }

    fn path_readlink(
        &self,
        fd: Fd,
        path: &str,
        buf: Ptr<u8>,
        mut buf_len: Ptr<Size>,
    ) -> StatusTrapResult {
        let file = self.abs_path(fd, path)?;
        let path_buf = std::fs::read_link(file)?;
        let bytes = path_buf.to_str().unwrap().as_bytes();
        if bytes.len() >= buf_len.value() as usize {
            return Status::Overflow.into();
        }
        buf.copy_slice(bytes)?;
        buf_len.set(bytes.len() as u32);

        Status::Success.into()
    }

    fn path_remove_directory(&self, fd: Fd, path: &str) -> StatusResult {
        let abs_path = self.abs_path(fd, path)?;
        self.remove_directory(abs_path)
    }

    fn path_rename(&self, fd: Fd, path: &str, new_fd: Fd, new_path: &str) -> StatusResult {
        let from = self.abs_path(fd, path)?;
        let to = self.abs_path(new_fd, new_path)?;
        self.rename(from, to)
    }

    fn path_symlink(&self, old_path: &str, fd: Fd, new_path: &str) -> StatusResult {
        let old = self.abs_path(fd, old_path)?;
        let new = self.abs_path(fd, new_path)?;
        platform_symlink(old, new)?;
        Status::Success.into()
    }

    fn path_unlink_file(&self, fd: Fd, path: &str) -> StatusResult {
        let file = self.abs_path(fd, path)?;
        std::fs::remove_file(file)?;
        Status::Success.into()
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
        // Ignore for now
        Status::Success
    }

    fn sock_send(
        &self,
        _fd: Fd,
        _si_data: &[IoSlice<'_>],
        _si_flags: u32,
        _ro_datalen_ptr: u32,
    ) -> Status {
        // Ignore for now
        Status::Success
    }

    fn sock_shutdown(&self, _fd: Fd, _how: u32) -> Status {
        // Ignore for now
        Status::Inval
    }
}
