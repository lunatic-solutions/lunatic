use dashmap::DashMap;
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
}

impl Environment {
    pub fn new(id: u64) -> Self {
        Self {
            environment_id: id,
            processes: Arc::new(DashMap::new()),
            next_process_id: Arc::new(AtomicU64::new(1)),
        }
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
}

#[derive(Clone, Default)]
pub struct Environments {
    envs: Arc<DashMap<u64, Environment>>,
}

impl Environments {
    pub fn get_or_create(&mut self, id: u64) -> Environment {
        if !self.envs.contains_key(&id) {
            let env = Environment::new(id);
            self.envs.insert(id, env.clone());
            env
        } else {
            self.envs.get(&id).map(|e| e.clone()).unwrap()
        }
    }
}
