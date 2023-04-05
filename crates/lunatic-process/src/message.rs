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

use smallvec::SmallVec;

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
    pub resources: SmallVec<[Option<Arc<Resource>>; 3]>,
}

impl DataMessage {
    /// Create a new message.
    pub fn new(tag: Option<i64>, buffer_capacity: usize) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer: Vec::with_capacity(buffer_capacity),
            resources: SmallVec::new(),
        }
    }

    /// Create a new message from a vec.
    pub fn new_from_vec(tag: Option<i64>, buffer: Vec<u8>) -> Self {
        Self {
            tag,
            read_ptr: 0,
            buffer,
            resources: SmallVec::new(),
        }
    }

    /// Adds a resource to the message and returns the index of it inside of the message.
    ///
    /// The resource is `Any` and is downcasted when accessing later.
    pub fn add_resource(&mut self, resource: Arc<Resource>) -> usize {
        self.resources.push(Some(resource));
        self.resources.len() - 1
    }

    /// Takes a resource from the message, downcasting to `T`.
    ///
    /// If the index is out of bounds, or the downcast fails, None is returned and the resource
    /// will remain in the message.
    pub fn take_resource<T: Send + Sync + 'static>(&mut self, index: usize) -> Option<Arc<T>> {
        let resource_ref = self.resources.get_mut(index)?;
        let resource_any = std::mem::take(resource_ref).map(|resource| resource.downcast())?;
        match resource_any {
            Ok(resource) => Some(resource),
            Err(resource) => {
                // Downcast failed, return the resource back to the message.
                *resource_ref = Some(resource);
                None
            }
        }
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
