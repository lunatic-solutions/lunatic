use crate::{
    channel::{api::ChannelState, ChannelReceiver, Message},
    module::LunaticModule,
};

use super::{FunctionLookup, MemoryChoice, Process};

use anyhow::Result;
use smol::{channel::bounded, future::yield_now, Timer};
use uptown_funk::{host_functions, state::HashMapStore, StateMarker};

use std::{
    mem::replace,
    time::{Duration, Instant},
};

pub struct ProcessState {
    module: LunaticModule,
    channel_state: ChannelState,
    pub processes: HashMapStore<Process>,
}

impl StateMarker for ProcessState {}

impl ProcessState {
    pub fn new(module: LunaticModule, channel_state: ChannelState) -> Self {
        Self {
            module,
            channel_state,
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

    // Suspend process for `millis`.
    async fn sleep_ms(&self, millis: u64) {
        let now = Instant::now();
        let when = now + Duration::from_millis(millis);
        Timer::at(when).await;
    }

    // Spawn a new process with a context and call a function from the function table by `index`.
    //
    // Once the process is created the context will be passed through a Channel::Receiver to it.
    async fn spawn_with_context(&self, index: u32, context: &[u8]) -> Process {
        let (sender, receiver) = bounded(1);
        let host_resources = &mut self
            .channel_state
            .inner
            .borrow_mut()
            .next_message_host_resources;
        let host_resources = replace(host_resources, Vec::new());
        let message = Message::new(context.as_ptr(), context.len(), host_resources);
        let _ignore = sender.send(message).await;

        Process::spawn(
            Some(ChannelReceiver::from(receiver)),
            self.module.clone(),
            FunctionLookup::TableIndex(index),
            MemoryChoice::New,
        )
    }

    // Wait on child process to finish.
    // Returns 0 if process didn't trap, otherwise 1
    async fn join(&self, process: Process) -> u32 {
        match process.task.await {
            Ok(_) => 0,
            Err(_) => 1,
        }
    }

    // Drops the Task and cancels the process
    //
    // It's currently not safe to cancel a process in Lunatic.
    // All processes are executed on a separate stack, but if we cancel it the stack memory
    // will be freed without actually unwinding it. This means that values and references
    // living on the separate stack will never be freed.
    async fn cancel_process(&self, _process: Process) {
        // _process will take ownership here of the underlying task and drop it.
        // See: https://docs.rs/smol/latest/smol/struct.Task.html
    }

    // Detaches process
    async fn detach_process(&self, process: Process) {
        process.task.detach()
    }
}
