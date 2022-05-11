use dashmap::DashMap;
use lunatic_distributed::{control::ControlInterface, distributed::DistributedInterface};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::{Process, Signal};

#[derive(Clone)]
pub struct Environment {
    environment_id: u64,
    next_process_id: Arc<AtomicU64>,
    processes: Arc<DashMap<u64, Arc<dyn Process>>>,
    control: Option<ControlInterface>,
    #[allow(unused)]
    distributed: Option<DistributedInterface>,
}

impl Environment {
    pub fn new(
        id: u64,
        control: Option<ControlInterface>,
        distributed: Option<DistributedInterface>,
    ) -> Self {
        Self {
            environment_id: id,
            processes: Arc::new(DashMap::new()),
            next_process_id: Arc::new(AtomicU64::new(1)),
            control,
            distributed,
        }
    }

    pub fn local() -> Self {
        Self::new(1, None, None)
    }

    pub fn get_process(&self, id: u64) -> Option<Arc<dyn Process>> {
        self.processes.get(&id).map(|x| x.clone())
    }

    pub fn add_process(&self, id: u64, proc: Arc<dyn Process>) {
        self.processes.insert(id, proc);
    }

    pub fn send(&self, id: u64, signal: Signal) {
        if let Some(proc) = self.processes.get(&id) {
            proc.send(signal);
        }
    }

    pub fn get_next_process_id(&self) -> u64 {
        self.next_process_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn id(&self) -> u64 {
        self.environment_id
    }

    pub fn node_id(&self) -> u64 {
        self.control.as_ref().map(|c| c.node_id).unwrap_or(1)
    }

    pub async fn get_module(&self, _module_id: u64) {
        todo!()
    }
}
