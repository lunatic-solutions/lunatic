/*!
The [`Message`] is a special variant of a [`Signal`](crate::Signal) that can be sent to
processes. The most common kind of Message is a [`DataMessage`], but there are also some special
kinds of messages, like the [`Message::LinkDied`], that is received if a linked process dies.
*/

use std::{
    any::Any,
    fmt::Debug,
    io::{Read, Write},
    sync::Arc,
};

use lunatic_networking_api::{TcpConnection, TlsConnection};
use tokio::net::UdpSocket;

use crate::runtimes::wasmtime::WasmtimeCompiledModule;

pub type Resource = dyn Any + Send + Sync;

/// Can be sent between processes by being embedded into a  [`Signal::Message`][0]
///
/// A [`Message`] has 2 variants:
/// * Data - Regular message containing a tag, buffer and resources.
/// * LinkDied - A `LinkDied` signal that was turned into a message.
///
/// [0]: crate::Signal
#[derive(Debug)]
pub enum Message {
    Data(DataMessage),
    LinkDied(Option<i64>),
}

impl Message {
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
pub struct DataMessage {
    // TODO: Only the Node implementation depends on these fields being public.
    pub tag: Option<i64>,
    pub read_ptr: usize,
    pub buffer: Vec<u8>,
    pub resources: Vec<Option<Arc<Resource>>>,
}

impl DataMessage {
    /// Create a new message.
    pub fn new(tag: Option<i64>, buffer_capacity: usize) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer: Vec::with_capacity(buffer_capacity),
            resources: Vec::new(),
        }
    }

    /// Create a new message from a vec.
    pub fn new_from_vec(tag: Option<i64>, buffer: Vec<u8>) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer,
            resources: Vec::new(),
        }
    }

    /// Adds a resource to the message and returns the index of it inside of the message.
    ///
    /// The resource is `Any` and is downcasted when accessing later.
    pub fn add_resource(&mut self, resource: Arc<Resource>) -> usize {
        self.resources.push(Some(resource));
        self.resources.len() - 1
    }

    // /// Takes a process from the message, but preserves the indexes of all others.
    // ///
    // /// If the index is out of bound or the resource is not a process the function will return
    // /// None.
    // pub fn take_process(&mut self, index: usize) -> Option<Arc<dyn Process>> {
    //     self.take_downcast(index)
    // }

    /// Takes a module from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a module the function will return
    /// None.
    pub fn take_module<T: 'static>(
        &mut self,
        index: usize,
    ) -> Option<Arc<WasmtimeCompiledModule<T>>> {
        self.take_downcast(index)
    }

    /// Takes a TCP stream from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_tcp_stream(&mut self, index: usize) -> Option<Arc<TcpConnection>> {
        self.take_downcast(index)
    }

    /// Takes a UDP Socket from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_udp_socket(&mut self, index: usize) -> Option<Arc<UdpSocket>> {
        self.take_downcast(index)
    }

    /// Takes a TLS stream from the message, but preserves the indexes of all others.
    ///
    /// If the index is out of bound or the resource is not a tcp stream the function will return
    /// None.
    pub fn take_tls_stream(&mut self, index: usize) -> Option<Arc<TlsConnection>> {
        self.take_downcast(index)
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

    fn take_downcast<T: Send + Sync + 'static>(&mut self, index: usize) -> Option<Arc<T>> {
        let resource = self.resources.get_mut(index);
        match resource {
            Some(resource_ref) => {
                let resource_any = std::mem::take(resource_ref).map(|resource| resource.downcast());
                match resource_any {
                    Some(Ok(resource)) => Some(resource),
                    Some(Err(resource)) => {
                        *resource_ref = Some(resource);
                        None
                    }
                    None => None,
                }
            }
            None => None,
        }
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
                "Reading outside message buffer",
            ));
        };
        let bytes = buf.write(slice)?;
        self.read_ptr += bytes;
        Ok(bytes)
    }
}
