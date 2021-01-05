use crate::module::LunaticModule;

use super::{FunctionLookup, MemoryChoice, Process};

use anyhow::Result;
use smol::{future::yield_now, Timer};
use uptown_funk::{host_functions, state::HashMapStore};

use std::time::{Duration, Instant};

pub struct ProcessState {
    module: LunaticModule,
    pub processes: HashMapStore<Process>,
}

impl ProcessState {
    pub fn new(module: LunaticModule) -> Self {
        Self {
            module,
            processes: HashMapStore::new(),
        }
    }
}

#[host_functions(namespace = "lunatic")]
impl ProcessState {
    // Yield this process allowing other to be scheduled on same thread.
    async fn yield_(&self) {
        yield_now().await
    }

    // Suspend process for some time
    async fn sleep_ms(&self, millis: i64) {
        let now = Instant::now();
        let when = now + Duration::from_millis(millis as u64);
        Timer::at(when).await;
    }

    // Spawn new process and call a fuction from the function table under the `index` and pass one u32 argument.
    async fn spawn(&self, index: u32, argument1: u32, argument2: u32) -> Process {
        Process::spawn(
            self.module.clone(),
            FunctionLookup::TableIndex((index, argument1, argument2)),
            MemoryChoice::New,
        )
    }

    // Wait on chaild process to finish.
    async fn join(&self, process: Process) {
        let _ = process.task.await;
    }
}
