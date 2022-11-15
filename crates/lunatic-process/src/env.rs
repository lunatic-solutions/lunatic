use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::{Process, Signal};

pub trait Environment: Send + Sync {
    fn id(&self) -> u64;
    fn get_next_process_id(&self) -> u64;
    fn get_process(&self, id: u64) -> Option<Arc<dyn Process>>;
    fn add_process(&self, id: u64, proc: Arc<dyn Process>);
    fn remove_process(&self, id: u64);
    fn process_count(&self) -> usize;
    fn send(&self, id: u64, signal: Signal);
}

pub trait Environments: Send + Sync {
    type Env: Environment;
    fn create(&self, id: u64) -> Arc<Self::Env>;
    fn get(&self, id: u64) -> Option<Arc<Self::Env>>;
}

#[derive(Clone)]
pub struct LunaticEnvironment {
    environment_id: u64,
    next_process_id: Arc<AtomicU64>,
    processes: Arc<DashMap<u64, Arc<dyn Process>>>,
}

impl LunaticEnvironment {
    pub fn new(id: u64) -> Self {
        Self {
            environment_id: id,
            processes: Arc::new(DashMap::new()),
            next_process_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl Environment for LunaticEnvironment {
    fn get_process(&self, id: u64) -> Option<Arc<dyn Process>> {
        self.processes.get(&id).map(|x| x.clone())
    }

    fn add_process(&self, id: u64, proc: Arc<dyn Process>) {
        self.processes.insert(id, proc);
        #[cfg(all(feature = "metrics", not(feature = "detailed_metrics")))]
        let labels: [(String, String); 0] = [];
        #[cfg(all(feature = "metrics", feature = "detailed_metrics"))]
        let labels = [("environment_id", self.id().to_string())];
        #[cfg(feature = "metrics")]
        metrics::gauge!(
            "lunatic.process.environment.process.count",
            self.processes.len() as f64,
            &labels
        );
    }

    fn remove_process(&self, id: u64) {
        self.processes.remove(&id);
        #[cfg(all(feature = "metrics", not(feature = "detailed_metrics")))]
        let labels: [(String, String); 0] = [];
        #[cfg(all(feature = "metrics", feature = "detailed_metrics"))]
        let labels = [("environment_id", self.id().to_string())];
        #[cfg(feature = "metrics")]
        metrics::gauge!(
            "lunatic.process.environment.process.count",
            self.processes.len() as f64,
            &labels
        );
    }

    fn process_count(&self) -> usize {
        self.processes.len()
    }

    fn send(&self, id: u64, signal: Signal) {
        if let Some(proc) = self.processes.get(&id) {
            proc.send(signal);
        }
    }

    fn get_next_process_id(&self) -> u64 {
        self.next_process_id.fetch_add(1, Ordering::Relaxed)
    }

    fn id(&self) -> u64 {
        self.environment_id
    }
}

#[derive(Clone, Default)]
pub struct LunaticEnvironments {
    envs: Arc<DashMap<u64, Arc<LunaticEnvironment>>>,
}

impl Environments for LunaticEnvironments {
    type Env = LunaticEnvironment;
    fn create(&self, id: u64) -> Arc<Self::Env> {
        let env = Arc::new(LunaticEnvironment::new(id));
        self.envs.insert(id, env.clone());
        #[cfg(feature = "metrics")]
        metrics::gauge!("lunatic.process.environment.count", self.envs.len() as f64);
        env
    }
    fn get(&self, id: u64) -> Option<Arc<Self::Env>> {
        self.envs.get(&id).map(|e| e.clone())
    }
}
