/*!
The [`Message`] is a special variant of a [`Signal`](crate::Signal) that can be sent to
processes. The most common kind of Message is a [`DataMessage`], but there are also some special
kinds of messages, like the [`Message::Signal`], that is received if a linked process dies.
*/

use std::{
    fmt::Debug,
    io::{Read, Write},
    num::NonZeroU64,
    sync::Arc,
};

use async_std::net::TcpStream;

use crate::Process;

/// Can be sent between processes by being embedded into a  [`Signal::Message`][0]
///
/// A [`Message`] has 2 variants:
/// * Data - Regular message containing a tag, buffer and resources.
/// * Signal - A signal (`LinkDied`) that was turned into a message.
///
/// [0]: crate::Signal
#[derive(Debug)]
pub enum Message {
    Data(DataMessage),
    Signal(Option<i64>),
}

impl Message {
    pub fn tag(&self) -> Option<i64> {
        match self {
            Message::Data(message) => message.tag,
            Message::Signal(tag) => *tag,
        }
    }

    pub fn id(&self) -> Option<NonZeroU64> {
        match self {
            Message::Data(message) => Some(message.id),
            Message::Signal(_) => None,
        }
    }
}

/// A variant of a [`Message`] that has a buffer of data and resources attached to it.
///
/// It implements the [`Read`](std::io::Read) and [`Write`](std::io::Write) traits.
#[derive(Debug)]
pub struct DataMessage {
    id: NonZeroU64,
    tag: Option<i64>,
    reply_id: Option<NonZeroU64>,
    read_ptr: usize,
    buffer: Vec<u8>,
    resources: Vec<Resource>,
}

impl DataMessage {
    /// Create a new message.
    pub fn new(id: NonZeroU64, tag: Option<i64>, buffer_capacity: usize) -> Self {
        Self {
            id,
            tag,
            reply_id: None,
            read_ptr: 0,
            buffer: Vec::with_capacity(buffer_capacity),
            resources: Vec::new(),
        }
    }

    /// Adds a process to the message and returns the index of it inside of the message
    pub fn add_process(&mut self, process: Arc<dyn Process>) -> usize {
        self.resources.push(Resource::Process(process));
        self.resources.len() - 1
    }

    /// Adds a TCP stream to the message and returns the index of it inside of the message
    pub fn add_tcp_stream(&mut self, tcp_stream: TcpStream) -> usize {
        self.resources.push(Resource::TcpStream(tcp_stream));
        self.resources.len() - 1
    }

    /// Takes a process from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a process the function will return
    /// None.
    pub fn take_process(&mut self, index: usize) -> Option<Arc<dyn Process>> {
        if let Some(resource_ref) = self.resources.get_mut(index) {
            let resource = std::mem::replace(resource_ref, Resource::None);
            match resource {
                Resource::Process(process) => {
                    return Some(process);
                }
                other => {
                    // Put the resource back if it's not a process and drop empty.
                    let _ = std::mem::replace(resource_ref, other);
                }
            }
        }
        None
    }

    /// Takes a TCP stream from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_tcp_stream(&mut self, index: usize) -> Option<TcpStream> {
        if let Some(resource_ref) = self.resources.get_mut(index) {
            let resource = std::mem::replace(resource_ref, Resource::None);
            match resource {
                Resource::TcpStream(stream) => {
                    return Some(stream);
                }
                other => {
                    // Put the resource back if it's not a tcp stream and drop empty.
                    let _ = std::mem::replace(resource_ref, other);
                }
            }
        }
        None
    }

    /// Moves read pointer to index.
    pub fn seek(&mut self, index: usize) {
        self.read_ptr = index;
    }

    pub fn size(&self) -> usize {
        self.buffer.len()
    }
}

impl Write for DataMessage {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for DataMessage {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        let slice = if let Some(slice) = self.buffer.get(self.read_ptr..) {
            slice
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Reading outside of message buffer",
            ));
        };
        let bytes = buf.write(slice)?;
        self.read_ptr += bytes;
        Ok(bytes)
    }
}

/// A resource ([`WasmProcess`](crate::WasmProcess), [`TcpStream`](async_std::net::TcpStream),
/// ...) that is attached to a [`DataMessage`].
pub enum Resource {
    None,
    Process(Arc<dyn Process>),
    TcpStream(TcpStream),
}

impl Debug for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Process(_) => write!(f, "Process"),
            Self::TcpStream(_) => write!(f, "TcpStream"),
        }
    }
}
