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

    pub fn add_process(&mut self, process: ProcessHandle) {
        self.resources.push(Resource::Process(process));
    }
}

#[derive(Debug)]
enum Resource {
    Process(ProcessHandle),
}
