pub mod creator;
pub mod channel;

use wasmer::{Module, Memory};
use async_wormhole::AsyncYielder;

use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, RwLock};
use std::future::Future;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::task::JoinHandle;
use thiserror::Error;

/// Each process uses a lot of virtual memory. Even vritual memory is cheap we need to have a hard
/// cap at around 20k processes or we risk to run out of virtual memory on a 64bit system.
pub const PROCESS_CAPACITY: usize = 20_000;

#[derive(Clone)]
pub struct Process {
    id: usize,
    pub module: Module,
    pub memory: Memory,
    receiver: Arc<Receiver<channel::ChannelBuffer>>,
    yielder: usize
}

impl Process {
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        let mut yielder = unsafe {
            std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<()>>)
        };
        yielder.async_suspend( future )
    }
}

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("instantation error")]
    Instantiation(#[from] wasmer::InstantiationError),
    #[error("heap allocation error")]
    HeapAllocation(#[from] wasmer::MemoryError),
    #[error("runtime error")]
    Runtime(#[from] wasmer::RuntimeError),
}
pub enum ProcessStatus {
    INIT,
    RUNNING,
    DONE(Result<(), ProcessError>)
}

pub struct ProcessInformation {
    status: ProcessStatus,
    sender: Option<Sender<channel::ChannelBuffer>>,
    join_handle: Option<JoinHandle<()>>
}

impl ProcessInformation {
    pub fn is_done(&self) -> bool {
        match self.status {
            ProcessStatus::DONE(_) => true,
            _ => false
        }
    }
}

/// Holds all active processes and some finished ones (before their slot is reused) in the system.
pub struct AllProcesses {
    /// All processes known to the system.
    pub processes: RwLock<Vec<Mutex<ProcessInformation>>>,
    /// Free slots in the `processes` vector for new processes.
    pub free_slots: RwLock<Vec<usize>>,
}

impl AllProcesses {
    /// Reserve a capacity of `PROCESS_CAPACITY` processes for this system.
    pub fn new() -> Self {
        Self {
            processes: RwLock::new(Vec::with_capacity(PROCESS_CAPACITY)),
            free_slots: RwLock::new((0..PROCESS_CAPACITY).rev().collect())
        }
    }
}

