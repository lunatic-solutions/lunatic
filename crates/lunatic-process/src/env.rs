use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::{Process, Signal};

pub struct Environment<T> {
    environment_id: u64,
    next_process_id: Arc<AtomicU64>,
    processes: Arc<DashMap<u64, Arc<dyn Process<T>>>>,
}

impl<T> Environment<T> {
    pub fn new(id: u64) -> Self {
        Self {
            environment_id: id,
            processes: Arc::new(DashMap::new()),
            next_process_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn get_process(&self, id: u64) -> Option<Arc<dyn Process<T>>> {
        self.processes.get(&id).map(|x| x.clone())
    }

    pub fn add_process(&self, id: u64, proc: Arc<dyn Process<T>>) {
        self.processes.insert(id, proc);
        #[cfg(all(feature = "metrics", not(feature = "detailed_metrics")))]
        let labels: [(String, String); 0] = [];
        #[cfg(all(feature = "metrics", feature = "detailed_metrics"))]
        let labels = [("environment_id", self.id().to_string())];

        metrics::gauge!(
            "lunatic.process.environment.process.count",
            self.processes.len() as f64,
            &labels
        );
    }

    pub fn remove_process(&self, id: u64) {
        self.processes.remove(&id);
        #[cfg(all(feature = "metrics", not(feature = "detailed_metrics")))]
        let labels: [(String, String); 0] = [];
        #[cfg(all(feature = "metrics", feature = "detailed_metrics"))]
        let labels = [("environment_id", self.id().to_string())];
        metrics::gauge!(
            "lunatic.process.environment.process.count",
            self.processes.len() as f64,
            &labels
        );
    }

    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    pub fn send(&self, id: u64, signal: Signal<T>) {
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

impl<T> Clone for Environment<T> {
    fn clone(&self) -> Self {
        Self {
            environment_id: self.environment_id,
            next_process_id: self.next_process_id.clone(),
            processes: self.processes.clone(),
        }
    }
}

pub struct Environments<T> {
    envs: Arc<DashMap<u64, Environment<T>>>,
}

impl<T> Environments<T> {
    pub fn get_or_create(&mut self, id: u64) -> Environment<T> {
        if !self.envs.contains_key(&id) {
            let env = Environment::new(id);
            self.envs.insert(id, env.clone());
            metrics::gauge!("lunatic.process.environment.count", self.envs.len() as f64);
            env
        } else {
            self.envs.get(&id).map(|e| e.clone()).unwrap()
        }
    }
}

impl<T> Clone for Environments<T> {
    fn clone(&self) -> Self {
        Self {
            envs: self.envs.clone(),
        }
    }
}

impl<T> Default for Environments<T> {
    fn default() -> Self {
        Self {
            envs: Default::default(),
        }
    }
}
