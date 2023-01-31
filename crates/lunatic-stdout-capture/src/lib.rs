use std::{
    any::Any,
    fmt::{Display, Formatter},
    io::{stdout, Cursor, IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex, RwLock},
};

use wasi_common::{
    file::{Advice, FdFlags, FileType, Filestat},
    Error, ErrorExt, SystemTimeSpec, WasiFile,
};

// This signature looks scary, but it just means that the vector holding all output streams
// is rarely extended and often accessed (`RwLock`). The `Mutex` is necessary to allow
// parallel writes for independent processes, it doesn't have any contention.
type StdOutVec = Arc<RwLock<Vec<Mutex<Cursor<Vec<u8>>>>>>;

/// `StdoutCapture` holds the standard output from multiple processes.
///
/// The most common pattern of usage is to capture together the output from a starting process
/// and all sub-processes. E.g. Hide output of sub-processes during testing.
#[derive(Clone, Debug)]
pub struct StdoutCapture {
    // If true, all captured writes are echoed to stdout. This is used in testing scenarios with
    // the flag `--nocapture` set, because we still need to capture the output to inspect panics.
    echo: bool,
    writers: StdOutVec,
    // Index of the stdout currently in use by a process
    index: usize,
}

impl PartialEq for StdoutCapture {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.writers, &other.writers) && self.index == other.index
    }
}

// Displays content of all processes contained inside `StdoutCapture`.
impl Display for StdoutCapture {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let streams = RwLock::read(&self.writers).unwrap();
        // If there is only one process, don't enumerate the output
        if streams.len() == 1 {
            write!(f, "{}", self.content()).unwrap();
        } else {
            for (i, stream) in streams.iter().enumerate() {
                writeln!(f, " --- process {i} stdout ---").unwrap();
                let stream = stream.lock().unwrap();
                let content = String::from_utf8_lossy(stream.get_ref()).to_string();
                write!(f, "{content}").unwrap();
            }
        }
        Ok(())
    }
}

impl StdoutCapture {
    // Create a new `StdoutCapture` with one stream inside.
    pub fn new(echo: bool) -> Self {
        Self {
            echo,
            writers: Arc::new(RwLock::new(vec![Mutex::new(Cursor::new(Vec::new()))])),
            index: 0,
        }
    }

    /// Returns `true` if this is the only reference to the outputs.
    pub fn only_reference(&self) -> bool {
        Arc::strong_count(&self.writers) == 1
    }

    /// Returns a clone of `StdoutCapture` pointing to the next stream
    pub fn next(&self) -> Self {
        let index = {
            let mut writers = RwLock::write(&self.writers).unwrap();
            // If the stream already exists don't add a new one, e.g. stdout & stderr share the same stream.
            writers.push(Mutex::new(Cursor::new(Vec::new())));
            writers.len() - 1
        };
        Self {
            echo: self.echo,
            writers: self.writers.clone(),
            index,
        }
    }

    /// Returns true if all streams are empty
    pub fn is_empty(&self) -> bool {
        let streams = RwLock::read(&self.writers).unwrap();
        streams.iter().all(|stream| {
            let stream = stream.lock().unwrap();
            stream.get_ref().is_empty()
        })
    }

    /// Returns stream's content
    pub fn content(&self) -> String {
        let streams = RwLock::read(&self.writers).unwrap();
        let stream = streams[self.index].lock().unwrap();
        String::from_utf8_lossy(stream.get_ref()).to_string()
    }

    /// Add string to end of the stream
    pub fn push_str(&self, content: &str) {
        let streams = RwLock::read(&self.writers).unwrap();
        let mut stream = streams[self.index].lock().unwrap();
        write!(stream, "{content}").unwrap();
    }
}

#[wiggle::async_trait]
impl WasiFile for StdoutCapture {
    fn as_any(&self) -> &dyn Any {
        self
    }
    async fn datasync(&mut self) -> Result<(), Error> {
        Ok(())
    }
    async fn sync(&mut self) -> Result<(), Error> {
        Ok(())
    }
    async fn get_filetype(&mut self) -> Result<FileType, Error> {
        Ok(FileType::Pipe)
    }
    async fn get_fdflags(&mut self) -> Result<FdFlags, Error> {
        Ok(FdFlags::APPEND)
    }
    async fn set_fdflags(&mut self, _fdflags: FdFlags) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn get_filestat(&mut self) -> Result<Filestat, Error> {
        Ok(Filestat {
            device_id: 0,
            inode: 0,
            filetype: self.get_filetype().await?,
            nlink: 0,
            size: 0, // XXX no way to get a size out of a Write :(
            atim: None,
            mtim: None,
            ctim: None,
        })
    }
    async fn set_filestat_size(&mut self, _size: u64) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn advise(&mut self, _offset: u64, _len: u64, _advice: Advice) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn allocate(&mut self, _offset: u64, _len: u64) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn read_vectored<'a>(&mut self, _bufs: &mut [IoSliceMut<'a>]) -> Result<u64, Error> {
        Err(Error::badf())
    }
    async fn read_vectored_at<'a>(
        &mut self,
        _bufs: &mut [IoSliceMut<'a>],
        _offset: u64,
    ) -> Result<u64, Error> {
        Err(Error::badf())
    }
    async fn write_vectored<'a>(&mut self, bufs: &[IoSlice<'a>]) -> Result<u64, Error> {
        let streams = RwLock::read(&self.writers).unwrap();
        let mut stream = streams[self.index].lock().unwrap();
        let n = stream.write_vectored(bufs)?;
        // Echo the captured part to stdout
        if self.echo {
            stream.seek(SeekFrom::End(-(n as i64)))?;
            let mut echo = vec![0; n];
            stream.read_exact(&mut echo)?;
            stdout().write_all(&echo)?;
        }
        Ok(n.try_into()?)
    }
    async fn write_vectored_at<'a>(
        &mut self,
        _bufs: &[IoSlice<'a>],
        _offset: u64,
    ) -> Result<u64, Error> {
        Err(Error::badf())
    }
    async fn seek(&mut self, _pos: SeekFrom) -> Result<u64, Error> {
        Err(Error::badf())
    }
    async fn peek(&mut self, _buf: &mut [u8]) -> Result<u64, Error> {
        Err(Error::badf())
    }
    async fn set_times(
        &mut self,
        _atime: Option<SystemTimeSpec>,
        _mtime: Option<SystemTimeSpec>,
    ) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn num_ready_bytes(&self) -> Result<u64, Error> {
        Ok(0)
    }
    fn isatty(&mut self) -> bool {
        false
    }
    async fn readable(&self) -> Result<(), Error> {
        Err(Error::badf())
    }
    async fn writable(&self) -> Result<(), Error> {
        Err(Error::badf())
    }

    async fn sock_accept(&mut self, _fdflags: FdFlags) -> Result<Box<dyn WasiFile>, Error> {
        Err(Error::badf())
    }
}
