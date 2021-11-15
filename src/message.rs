/*!
The [`Message`] is a special variant of a [`Signal`](crate::Signal) that can be sent to
processes. The most common kind of Message is a [`DataMessage`], but there are also some special
kinds of messages, like the [`Message::Signal`], that is received if a linked process dies.
*/

use std::{fmt::Debug, io::Write, num::NonZeroU64, sync::Arc};

use async_std::net::TcpStream;

use crate::{process::ProcessId, Process};

/// Can be sent between processes by being embedded into a  [`Signal::Message`][0]
/// It has a buffer of data and resources attached to it.
///
/// It implements the [`Read`](std::io::Read) and [`Write`](std::io::Write) traits.
#[derive(Debug)]
pub struct Message {
    id: NonZeroU64,
    process_id: ProcessId,
    is_signal: bool,
    reply_id: Option<NonZeroU64>,
    pub data: Vec<u8>,
    resources: Vec<Resource>,
}

impl Message {
    /// Create a new message.
    pub fn new(id: NonZeroU64, process_id: ProcessId) -> Self {
        Self {
            id,
            is_signal: false,
            process_id,
            reply_id: None,
            data: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Create a new message with signal set
    pub fn new_signal(id: NonZeroU64, process_id: ProcessId) -> Self {
        Self {
            id,
            is_signal: true,
            process_id,
            reply_id: None,
            data: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub fn id(&self) -> NonZeroU64 {
        self.id
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub fn is_signal(&self) -> bool {
        self.is_signal
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

    pub fn set_reply(&mut self, reply_id: NonZeroU64) {
        self.reply_id = Some(reply_id);
    }

    pub fn is_reply(&self) -> bool {
        self.reply_id.is_some()
    }

    pub fn is_reply_equal(&self, reply_id: NonZeroU64) -> bool {
        self.reply_id.map(|r| r == reply_id).unwrap_or(false)
    }
}

impl Write for Message {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.data.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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

pub struct ReadingMessage {
    pub message: Message,
    pub seek_ptr: usize,
}

impl ReadingMessage {
    pub fn new(message: Message) -> Self {
        Self {
            message,
            seek_ptr: 0,
        }
    }
}
