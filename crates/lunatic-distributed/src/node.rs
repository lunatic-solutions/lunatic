use std::sync::Arc;

use dashmap::DashMap;
use lunatic_process::env::Environment;

#[derive(Clone)]
pub struct Node {
    inner: Arc<InnerNode>,
}

struct InnerNode {
    envs: DashMap<u64, Environment>,
}

#[allow(clippy::new_without_default)]
impl Node {
    pub fn new() -> Node {
        Node {
            inner: Arc::new(InnerNode {
                envs: DashMap::new(),
            }),
        }
    }

    pub fn add_env(&self, id: u64, env: Environment) {
        self.inner.envs.insert(id, env);
    }

    pub fn env(&self, id: u64) -> Option<Environment> {
        self.inner.envs.get(&id).map(|e| e.clone())
    }
}
