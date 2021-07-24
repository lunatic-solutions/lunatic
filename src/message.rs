use std::sync::Arc;

use tokio::{net::TcpStream, sync::Mutex};

use crate::process::ProcessHandle;

/// Messages can be sent between processes.
///
/// A [`Message`] can have 2 types:
/// * Data - Regular message containing a buffer and resources.
/// * Signal - A Signal that was received from another process, but was turned into a message.
#[derive(Debug)]
pub enum Message {
    Data(DataMessage),
    Signal,
}

impl Default for Message {
    fn default() -> Self {
        Message::Data(DataMessage::default())
    }
}

/// Messages consist of two parts:
/// * buffer - raw data
/// * resources - like [`ProcessHandle`] or `TcpStream`
#[derive(Debug, Default)]
pub struct DataMessage {
    buffer: Vec<u8>,
    resources: Vec<Resource>,
}

impl DataMessage {
    /// Create a new message.
    pub fn new(buffer: Vec<u8>, resources: Vec<Resource>) -> Self {
        Self { buffer, resources }
    }

    pub fn set_buffer(&mut self, buffer: Vec<u8>) {
        self.buffer = buffer;
    }

    pub fn buffer(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    pub fn add_process(&mut self, process: ProcessHandle) -> usize {
        self.resources.push(Resource::Process(process));
        self.resources.len() - 1
    }

    pub fn add_tcp_stream(&mut self, tcp_stream: Arc<Mutex<TcpStream>>) -> usize {
        self.resources.push(Resource::TcpStream(tcp_stream));
        self.resources.len() - 1
    }

    pub fn resources(self) -> Vec<Resource> {
        self.resources
    }

    pub fn resources_size(&self) -> usize {
        self.resources.len()
    }
}

#[derive(Debug)]
pub enum Resource {
    Process(ProcessHandle),
    TcpStream(Arc<Mutex<TcpStream>>),
}
