use super::types::{Filestat, OpenFlags, Status, StatusResult};
use uptown_funk::StateMarker;
use wasi_common::{WasiCtx, WasiCtxBuilder};

use std::{
    fs,
    fs::{File, OpenOptions},
    io::{IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::SystemTime,
    u32,
};

type Fd = u32;

pub struct WasiState {
    pub ctx: WasiCtx,
    fds: Vec<Option<FileDesc>>,
}

// TODO: create AbsPath

impl WasiState {
    pub fn new() -> Self {
        // TODO cannot trap if open / fails
        let ctx = WasiCtxBuilder::new().build().unwrap();
        Self {
            ctx,
            fds: vec![None, None, None, FileDesc::open("/").ok()],
        }
    }

    fn get_mut_file_desc(&mut self, fd: Fd) -> Result<&mut FileDesc, Status> {
        self.fds
            .get_mut(fd as usize)
            .ok_or(Status::Badf)?
            .as_mut()
            .ok_or(Status::Badf)
    }

    fn get_file_desc(&self, fd: Fd) -> Result<&FileDesc, Status> {
        self.fds
            .get(fd as usize)
            .ok_or(Status::Badf)?
            .as_ref()
            .ok_or(Status::Badf)
    }

    pub fn abs_path(&self, from: Fd, rel_path: &str) -> Result<PathBuf, Status> {
        let f = self.get_file_desc(from)?;
        Ok(f.path.join(rel_path).into())
    }

    pub fn get_path(&self, from: Fd) -> Result<PathBuf, Status> {
        let f = self.get_file_desc(from)?;
        Ok(f.path.clone())
    }

    pub fn open<P: AsRef<Path>>(&mut self, abs_path: P, flags: OpenFlags) -> Result<Fd, Status> {
        self.fds
            .push(Some(FileDesc::open_with_flags(abs_path, flags)?));
        Ok(self.fds.len() as u32 - 1)
    }

    pub fn write(&mut self, fd: Fd, ciovs: &[IoSlice<'_>]) -> Result<usize, Status> {
        let f = self.get_mut_file_desc(fd)?;
        Ok(f.file.write_vectored(ciovs)?)
    }

    pub fn read(&mut self, fd: Fd, iovs: &mut [IoSliceMut<'_>]) -> Result<usize, Status> {
        let f = self.get_mut_file_desc(fd)?;
        Ok(f.file.read_vectored(iovs)?)
    }

    pub fn close(&mut self, fd: Fd) {
        self.fds[fd as usize] = None;
    }

    pub fn tell(&mut self, fd: Fd) -> Result<u64, Status> {
        let f = self.get_mut_file_desc(fd)?;
        Ok(f.file.seek(SeekFrom::Current(0))?)
    }

    pub fn seek(&mut self, fd: Fd, seek_from: SeekFrom) -> Result<u64, Status> {
        let f = self.get_mut_file_desc(fd)?;
        Ok(f.file.seek(seek_from)?)
    }

    pub fn create_directory<P: AsRef<Path>>(&self, abs_path: P) -> StatusResult {
        Ok(fs::create_dir(abs_path)?)
    }

    pub fn remove_directory<P: AsRef<Path>>(&self, abs_path: P) -> StatusResult {
        Ok(fs::remove_dir(abs_path)?)
    }

    pub fn rename<P: AsRef<Path>>(&self, abs_from: P, abs_to: P) -> StatusResult {
        Ok(fs::rename(abs_from, abs_to)?)
    }

    pub fn filestat(&self, fd: Fd) -> Result<Filestat, Status> {
        if let Some(Some(f)) = self.fds.get(fd as usize) {
            let metadata = fs::metadata(&f.path)?;
            // TODO repeated and not sure how timestamp is actually represented
            let atim = metadata
                .accessed()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mtim = metadata
                .modified()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let ctim = metadata
                .created()
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(Filestat {
                dev: 0,
                ino: 0,
                filetype: metadata.file_type().into(),
                nlink: 0,
                size: metadata.len(),
                atim,
                mtim,
                ctim,
            })
        } else {
            Err(Status::Badf)
        }
    }

    pub fn filestat_path<P: AsRef<Path>>(&self, abs_path: P) -> Result<Filestat, Status> {
        let metadata = fs::metadata(&abs_path)?;
        let atim = metadata
            .accessed()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mtim = metadata
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ctim = metadata
            .created()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok(Filestat {
            dev: 0,
            ino: 0,
            filetype: metadata.file_type().into(),
            nlink: 0,
            size: metadata.len(),
            atim,
            mtim,
            ctim,
        })
    }

    pub fn set_size(&mut self, fd: Fd, len: u64) -> Result<(), Status> {
        let f = self.get_mut_file_desc(fd)?;
        f.file.set_len(len)?;
        Ok(())
    }
}

impl StateMarker for WasiState {}

#[derive(Debug)]
struct FileDesc {
    pub file: File,
    pub path: PathBuf,
}

impl FileDesc {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self, Status> {
        let file = File::open(&path)?;
        let path = PathBuf::from(path.as_ref());
        Ok(Self { file, path })
    }

    fn open_with_flags<P: AsRef<Path>>(path: P, flags: OpenFlags) -> Result<Self, Status> {
        if fs::metadata(&path)?.is_dir() {
            return Self::open(&path);
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(flags.create())
            .truncate(flags.truncate())
            .open(&path)?;
        let path = PathBuf::from(path.as_ref());
        Ok(Self { file, path })
    }
}
