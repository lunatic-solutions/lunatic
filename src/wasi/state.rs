use super::types::OpenFlags;
use uptown_funk::StateMarker;
use wasi_common::{WasiCtx, WasiCtxBuilder};

use std::{fs::{File, OpenOptions}, io::{IoSlice, IoSliceMut, Read, Write}, path::{Path, PathBuf}, u32};

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
