use crate::module::LunaticModule;

use super::{FunctionLookup, MemoryChoice, Process};

use anyhow::Result;
use smol::{future::yield_now, Timer};
use uptown_funk::host_functions;

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct ProcessState {
    module: LunaticModule,
    count: RefCell<i32>,
    state: RefCell<HashMap<i32, Process>>,
}

impl ProcessState {
    pub fn new(module: LunaticModule) -> Self {
        Self {
            module,
            count: RefCell::new(0),
            state: RefCell::new(HashMap::new()),
        }
    }

    pub fn add_process(&self, channel: Process) -> i32 {
        let mut id = self.count.borrow_mut();
        *id += 1;
        self.state.borrow_mut().insert(*id, channel);
        *id
    }

    pub fn remove_process(&self, id: i32) -> Option<Process> {
        self.state.borrow_mut().remove(&id)
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

    // Spawn new process and call a fuction from the function table under the `index` and pass one i32 argument.
    async fn spawn(&self, index: i32, argument1: i32, argument2: i64) -> Process {
        Process::spawn(
            self.module.clone(),
            FunctionLookup::TableIndex((index, argument1, argument2)),
            MemoryChoice::New,
        )
    }

    // Wait on chaild process to finish.
    async fn join(&self, process: Process) {
        process.take_task().unwrap().await.unwrap();
    }
}
