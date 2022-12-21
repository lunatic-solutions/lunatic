use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    future::Future,
    time::{Duration, Instant},
};

use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_common_api::IntoTrap;
use lunatic_process::{state::ProcessState, Signal};
use lunatic_process_api::ProcessCtx;
use tokio::task::JoinHandle;
use wasmtime::{Caller, Linker};

#[derive(Debug)]
struct HeapValue {
    instant: Instant,
    key: u64,
}

impl PartialOrd for HeapValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.instant.cmp(&other.instant).reverse())
    }
}

impl Ord for HeapValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.instant.cmp(&other.instant).reverse()
    }
}

impl PartialEq for HeapValue {
    fn eq(&self, other: &Self) -> bool {
        self.instant.eq(&other.instant)
    }
}

impl Eq for HeapValue {}

#[derive(Debug, Default)]
pub struct TimerResources {
    hash_map: HashMapId<JoinHandle<()>>,
    heap: BinaryHeap<HeapValue>,
}

impl TimerResources {
    pub fn add(&mut self, handle: JoinHandle<()>, target_time: Instant) -> u64 {
        self.cleanup_expired_timers();

        let id = self.hash_map.add(handle);
        self.heap.push(HeapValue {
            instant: target_time,
            key: id,
        });
        id
    }

    fn cleanup_expired_timers(&mut self) {
        let deadline = Instant::now();
        while let Some(HeapValue { instant, .. }) = self.heap.peek() {
            if *instant > deadline {
                // instant is after the deadline so stop
                return;
            }

            let key = self
                .heap
                .pop()
                .expect("not empty because we matched on peek")
                .key;
            self.hash_map.remove(key);
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<JoinHandle<()>> {
        self.hash_map.remove(id)
    }
}

pub trait TimerCtx {
    fn timer_resources(&self) -> &TimerResources;
    fn timer_resources_mut(&mut self) -> &mut TimerResources;
}

pub fn register<T: ProcessState + ProcessCtx<T> + TimerCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap("lunatic::timer", "send_after", send_after)?;
    linker.func_wrap1_async("lunatic::timer", "cancel_timer", cancel_timer)?;

    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.timers.started",
        metrics::Unit::Count,
        "number of timers set since startup, will usually be completed + canceled + active"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.timers.completed",
        metrics::Unit::Count,
        "number of timers completed since startup"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_counter!(
        "lunatic.timers.canceled",
        metrics::Unit::Count,
        "number of timers canceled since startup"
    );
    #[cfg(feature = "metrics")]
    metrics::describe_gauge!(
        "lunatic.timers.active",
        metrics::Unit::Count,
        "number of timers currently active"
    );

    Ok(())
}

// Sends the message to a process after a delay.
//
// There are no guarantees that the message will be received.
//
// Traps:
// * If the process ID doesn't exist.
// * If it's called before creating the next message.
fn send_after<T: ProcessState + ProcessCtx<T> + TimerCtx>(
    mut caller: Caller<T>,
    process_id: u64,
    delay: u64,
) -> Result<u64> {
    let message = caller
        .data_mut()
        .message_scratch_area()
        .take()
        .or_trap("lunatic::message::send_after")?;

    let process = caller.data_mut().environment().get_process(process_id);

    let target_time = Instant::now() + Duration::from_millis(delay);
    let timer_handle = tokio::task::spawn(async move {
        #[cfg(feature = "metrics")]
        metrics::increment_counter!("lunatic.timers.started");
        #[cfg(feature = "metrics")]
        metrics::increment_gauge!("lunatic.timers.active", 1.0);
        let duration_remaining = target_time - Instant::now();
        if duration_remaining != Duration::ZERO {
            tokio::time::sleep(duration_remaining).await;
        }
        if let Some(process) = process {
            #[cfg(feature = "metrics")]
            metrics::increment_counter!("lunatic.timers.completed");
            #[cfg(feature = "metrics")]
            metrics::decrement_gauge!("lunatic.timers.active", 1.0);
            process.send(Signal::Message(message));
        }
    });

    let id = caller
        .data_mut()
        .timer_resources_mut()
        .add(timer_handle, target_time);
    Ok(id)
}

// Cancels the specified timer.
//
// Returns:
// * 1 if a timer with the timer_id was found
// * 0 if no timer was found, this can be either because:
//     - timer had expired
//     - timer already had been canceled
//     - timer_id never corresponded to a timer
fn cancel_timer<T: ProcessState + TimerCtx + Send>(
    mut caller: Caller<T>,
    timer_id: u64,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        let timer_handle = caller.data_mut().timer_resources_mut().remove(timer_id);
        match timer_handle {
            Some(timer_handle) => {
                timer_handle.abort();
                #[cfg(feature = "metrics")]
                metrics::increment_counter!("lunatic.timers.canceled");
                #[cfg(feature = "metrics")]
                metrics::decrement_gauge!("lunatic.timers.active", 1.0);
                Ok(1)
            }
            None => Ok(0),
        }
    })
}
