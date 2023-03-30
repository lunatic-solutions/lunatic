use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::time::{interval, Interval};

use crate::{Process, Signal};

#[async_trait]
pub trait Environment: Send + Sync {
    fn id(&self) -> u64;
    fn get_next_process_id(&self) -> u64;
    fn get_process(&self, id: u64) -> Option<Arc<dyn Process>>;
    fn add_process(&self, id: u64, proc: Arc<dyn Process>);
    fn remove_process(&self, id: u64);
    fn process_count(&self) -> usize;
    async fn can_spawn_next_process(&self) -> Result<Option<()>>;
    fn send(&self, id: u64, signal: Signal);
    fn shutdown(&self) -> ShutdownFuture<'_, Self>
    where
        Self: Sized;
}

#[async_trait]
pub trait Environments: Send + Sync {
    type Env: Environment;

    async fn create(&self, id: u64) -> Arc<Self::Env>;
    async fn get(&self, id: u64) -> Option<Arc<Self::Env>>;
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

#[async_trait]
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

    async fn can_spawn_next_process(&self) -> Result<Option<()>> {
        // Don't impose any limits to process spawning
        Ok(Some(()))
    }

    fn shutdown(&self) -> ShutdownFuture<'_, Self>
    where
        Self: Sized,
    {
        for proc in self.processes.iter() {
            proc.send(Signal::Kill);
        }

        ShutdownFuture::new(self)
    }
}

#[derive(Clone, Default)]
pub struct LunaticEnvironments {
    envs: Arc<DashMap<u64, Arc<LunaticEnvironment>>>,
}

#[async_trait]
impl Environments for LunaticEnvironments {
    type Env = LunaticEnvironment;
    async fn create(&self, id: u64) -> Arc<Self::Env> {
        let env = Arc::new(LunaticEnvironment::new(id));
        self.envs.insert(id, env.clone());
        #[cfg(feature = "metrics")]
        metrics::gauge!("lunatic.process.environment.count", self.envs.len() as f64);
        env
    }

    async fn get(&self, id: u64) -> Option<Arc<Self::Env>> {
        self.envs.get(&id).map(|e| e.clone())
    }
}

#[derive(Debug)]
pub struct ShutdownFuture<'a, E> {
    environment: &'a E,
    interval: Interval,
}

impl<'a, E> ShutdownFuture<'a, E> {
    fn new(environment: &'a E) -> Self {
        ShutdownFuture {
            environment,
            interval: interval(tokio::time::Duration::from_millis(50)),
        }
    }
}

impl<'a, E> Future for ShutdownFuture<'a, E>
where
    E: Environment,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.environment.process_count() == 0 {
            Poll::Ready(())
        } else {
            match self.interval.poll_tick(cx) {
                Poll::Ready(_) => {
                    if self.environment.process_count() == 0 {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
