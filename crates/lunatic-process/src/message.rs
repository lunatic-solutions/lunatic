/*!
The [`Message`] is a special variant of a [`Signal`](crate::Signal) that can be sent to
processes. The most common kind of Message is a [`DataMessage`], but there are also some special
kinds of messages, like the [`Message::LinkDied`], that is received if a linked process dies.
*/

use std::{
    fmt::Debug,
    io::{Read, Write},
    sync::Arc,
};

use lunatic_networking_api::{TcpConnection, TlsConnection};
use tokio::net::UdpSocket;

use crate::{runtimes::wasmtime::WasmtimeCompiledModule, Process};

/// Can be sent between processes by being embedded into a  [`Signal::Message`][0]
///
/// A [`Message`] has 2 variants:
/// * Data - Regular message containing a tag, buffer and resources.
/// * LinkDied - A `LinkDied` signal that was turned into a message.
///
/// [0]: crate::Signal
#[derive(Debug)]
pub enum Message<T> {
    Data(DataMessage<T>),
    LinkDied(Option<i64>),
}

impl<T> Message<T> {
    pub fn tag(&self) -> Option<i64> {
        match self {
            Message::Data(message) => message.tag,
            Message::LinkDied(tag) => *tag,
        }
    }

    #[cfg(feature = "metrics")]
    pub fn write_metrics(&self) {
        match self {
            Message::Data(message) => message.write_metrics(),
            Message::LinkDied(_) => {
                metrics::increment_counter!("lunatic.process.messages.link_died.count");
            }
        }
    }
}

/// A variant of a [`Message`] that has a buffer of data and resources attached to it.
///
/// It implements the [`Read`](std::io::Read) and [`Write`](std::io::Write) traits.
#[derive(Debug, Default)]
pub struct DataMessage<T> {
    // TODO: Only the Node implementation depends on these fields being public.
    pub tag: Option<i64>,
    pub read_ptr: usize,
    pub buffer: Vec<u8>,
    pub resources: Vec<Resource<T>>,
}

impl<T> DataMessage<T> {
    /// Create a new message.
    pub fn new(tag: Option<i64>, buffer_capacity: usize) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer: Vec::with_capacity(buffer_capacity),
            resources: Vec::new(),
        }
    }

    /// Create a new message from a vec
    pub fn new_from_vec(tag: Option<i64>, buffer: Vec<u8>) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer,
            resources: Vec::new(),
        }
    }

    /// Adds a process to the message and returns the index of it inside of the message
    pub fn add_process(&mut self, process: Arc<dyn Process<T>>) -> usize {
        self.resources.push(Resource::Process(process));
        self.resources.len() - 1
    }

    /// Adds a module to the message and returns the index of it inside of the message
    pub fn add_module(&mut self, module: Arc<WasmtimeCompiledModule<T>>) -> usize {
        self.resources.push(Resource::Module(module));
        self.resources.len() - 1
    }

    /// Adds a TCP stream to the message and returns the index of it inside of the message
    pub fn add_tcp_stream(&mut self, tcp_stream: Arc<TcpConnection>) -> usize {
        self.resources.push(Resource::TcpStream(tcp_stream));
        self.resources.len() - 1
    }

    /// Adds a UDP socket to the message and returns the index of it inside of the message
    pub fn add_udp_socket(&mut self, udp_socket: Arc<UdpSocket>) -> usize {
        self.resources.push(Resource::UdpSocket(udp_socket));
        self.resources.len() - 1
    }

    /// Adds a TLS stream to the message and returns the index of it inside of the message
    pub fn add_tls_stream(&mut self, tls_stream: Arc<TlsConnection>) -> usize {
        self.resources.push(Resource::TlsStream(tls_stream));
        self.resources.len() - 1
    }

    /// Takes a process from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a process the function will return
    /// None.
    pub fn take_process(&mut self, index: usize) -> Option<Arc<dyn Process<T>>> {
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

    /// Takes a module from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a module the function will return
    /// None.
    pub fn take_module(&mut self, index: usize) -> Option<Arc<WasmtimeCompiledModule<T>>> {
        if let Some(resource_ref) = self.resources.get_mut(index) {
            let resource = std::mem::replace(resource_ref, Resource::None);
            match resource {
                Resource::Module(module) => {
                    return Some(module);
                }
                other => {
                    // Put the resource back if it's not a tcp stream and drop empty.
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
    pub fn take_tcp_stream(&mut self, index: usize) -> Option<Arc<TcpConnection>> {
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

    /// Takes a UDP Socket from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_udp_socket(&mut self, index: usize) -> Option<Arc<UdpSocket>> {
        if let Some(resource_ref) = self.resources.get_mut(index) {
            let resource = std::mem::replace(resource_ref, Resource::None);
            match resource {
                Resource::UdpSocket(socket) => {
                    return Some(socket);
                }
                other => {
                    // Put the resource back if it's not a tcp stream and drop empty.
                    let _ = std::mem::replace(resource_ref, other);
                }
            }
        }
        None
    }

    /// Takes a TLS stream from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_tls_stream(&mut self, index: usize) -> Option<Arc<TlsConnection>> {
        if let Some(resource_ref) = self.resources.get_mut(index) {
            let resource = std::mem::replace(resource_ref, Resource::None);
            match resource {
                Resource::TlsStream(stream) => {
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

    #[cfg(feature = "metrics")]
    pub fn write_metrics(&self) {
        metrics::increment_counter!("lunatic.process.messages.data.count");
        metrics::histogram!(
            "lunatic.process.messages.data.resources.count",
            self.resources.len() as f64
        );
        metrics::histogram!("lunatic.process.messages.data.size", self.size() as f64);
    }
}

impl<T> Write for DataMessage<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<T> Read for DataMessage<T> {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        let slice = if let Some(slice) = self.buffer.get(self.read_ptr..) {
            slice
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "Reading outside message buffer",
            ));
        };
        let bytes = buf.write(slice)?;
        self.read_ptr += bytes;
        Ok(bytes)
    }
}

/// A resource ([`WasmProcess`](crate::WasmProcess), [`TcpStream`](tokio::net::TcpStream),
/// ...) that is attached to a [`DataMessage`].
pub enum Resource<T> {
    None,
    Process(Arc<dyn Process<T>>),
    Module(Arc<WasmtimeCompiledModule<T>>),
    TcpStream(Arc<TcpConnection>),
    UdpSocket(Arc<UdpSocket>),
    TlsStream(Arc<TlsConnection>),
}

impl<T> Debug for Resource<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Process(_) => write!(f, "Process"),
            Self::Module(_) => write!(f, "Module"),
            Self::TcpStream(_) => write!(f, "TcpStream"),
            Self::UdpSocket(_) => write!(f, "UdpSocket"),
            Self::TlsStream(_) => write!(f, "TlsStream"),
        }
    }
}
