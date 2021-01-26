use super::types::{Filestat, OpenFlags, Status};
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
        let ctx = WasiCtxBuilder::new().build().unwrap();
        Self {
            ctx,
            fds: vec![None, None, None, FileDesc::open("/")],
        }
    }

    pub fn abs_path(&self, from: Fd, rel_path: &str) -> Option<PathBuf> {
        let f = self.fds.get(from as usize)?.as_ref()?;
        Some(f.path.join(rel_path).into())
    }

    pub fn get_path(&self, from: Fd) -> Option<PathBuf> {
        let f = self.fds.get(from as usize)?.as_ref()?;
        Some(f.path.clone())
    }

    pub fn open<P: AsRef<Path>>(&mut self, abs_path: P, flags: OpenFlags) -> Fd {
        self.fds.push(FileDesc::open_with_flags(abs_path, flags));
        self.fds.len() as u32 - 1
    }

    pub fn write(&mut self, fd: Fd, ciovs: &[IoSlice<'_>]) -> Option<Fd> {
        let f = self.fds.get_mut(fd as usize)?.as_mut()?;
        Some(f.file.write_vectored(ciovs).unwrap() as u32)
    }

    pub fn read(&mut self, fd: Fd, iovs: &mut [IoSliceMut<'_>]) -> Option<Fd> {
        let f = self.fds.get_mut(fd as usize)?.as_mut()?;
        Some(f.file.read_vectored(iovs).unwrap() as u32)
    }

    pub fn close(&mut self, fd: Fd) {
        self.fds[fd as usize] = None;
    }

    pub fn tell(&mut self, fd: Fd) -> Option<u64> {
        let f = self.fds.get_mut(fd as usize)?.as_mut()?;
        f.file.seek(SeekFrom::Current(0)).ok()
    }

    pub fn seek(&mut self, fd: Fd, seek_from: SeekFrom) -> Option<u64> {
        let f = self.fds.get_mut(fd as usize)?.as_mut()?;
        f.file.seek(seek_from).ok()
    }

    pub fn create_directory<P: AsRef<Path>>(&self, abs_path: P) -> Status {
        fs::create_dir(abs_path).into()
    }

    pub fn remove_directory<P: AsRef<Path>>(&self, abs_path: P) -> Status {
        fs::remove_dir(abs_path).into()
    }

    pub fn rename<P: AsRef<Path>>(&self, abs_from: P, abs_to: P) -> Status {
        fs::rename(abs_from, abs_to).into()
    }

    pub fn filestat(&self, fd: Fd) -> Option<Filestat> {
        if let Some(Some(f)) = self.fds.get(fd as usize) {
            let metadata = fs::metadata(&f.path).ok()?;
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
            Some(Filestat {
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
            None
        }
    }

    pub fn filestat_path<P: AsRef<Path>>(&self, abs_path: P) -> Option<Filestat> {
        let metadata = fs::metadata(&abs_path).ok()?;
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
        Some(Filestat {
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

    pub fn set_size(&mut self, fd: Fd, len: u64) -> Option<()> {
        let f = self.fds.get_mut(fd as usize)?.as_mut()?;
        f.file.set_len(len).ok();
        Some(())
    }
}

impl StateMarker for WasiState {}

#[derive(Debug)]
struct FileDesc {
    pub file: File,
    pub path: PathBuf,
}

impl FileDesc {
    fn open<P: AsRef<Path>>(path: P) -> Option<Self> {
        let file = File::open(&path).ok()?;
        let path = PathBuf::from(path.as_ref());
        Some(Self { file, path })
    }

    fn open_with_flags<P: AsRef<Path>>(path: P, flags: OpenFlags) -> Option<Self> {
        if fs::metadata(&path).unwrap().is_dir() {
            return Self::open(&path);
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(flags.create())
            .truncate(flags.truncate())
            .open(&path)
            .ok()?;
        let path = PathBuf::from(path.as_ref());
        Some(Self { file, path })
    }
}
