use std::sync::Arc;

use tokio::{net::TcpStream, sync::Mutex};

use crate::process::ProcessHandle;

#[derive(Debug, Default)]
pub struct Message {
    buffer: Vec<u8>,
    resources: Vec<Resource>,
}

impl Message {
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
}

#[derive(Debug)]
pub enum Resource {
    Process(ProcessHandle),
    TcpStream(Arc<Mutex<TcpStream>>),
}
