use std::sync::Arc;

use tokio::{net::TcpStream, sync::Mutex};

use crate::process::ProcessHandle;

/// A message that can be sent to a `process.
///
/// Messages consist of two parts:
/// * buffer - raw data
/// * resources - like [`ProcessHandle`] or `TcpStream`
#[derive(Debug, Default)]
pub struct Message {
    buffer: Vec<u8>,
    resources: Vec<Resource>,
}

impl Message {
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
