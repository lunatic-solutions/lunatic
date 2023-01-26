use std::sync::Arc;

use anyhow::Result;
use log::trace;
use tokio::task::JoinHandle;
use wasmtime::{ResourceLimiter, Val};

use crate::env::Environment;
use crate::runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime};
use crate::state::ProcessState;
use crate::{Process, Signal, WasmProcess};

/// Spawns a new wasm process from a compiled module.
///
/// A `Process` is created from a `module`, entry `function`, array of arguments and config. The
/// configuration will define some characteristics of the process, such as maximum memory, fuel
/// and host function properties (filesystem access, networking, ..).
///
/// After it's spawned the process will keep running in the background. A process can be killed
/// with `Signal::Kill` signal. If you would like to block until the process is finished you can
/// `.await` on the returned `JoinHandle<()>`.
pub async fn spawn_wasm<S>(
    env: Arc<dyn Environment>,
    runtime: WasmtimeRuntime,
    module: &WasmtimeCompiledModule<S>,
    state: S,
    function: &str,
    params: Vec<Val>,
    link: Option<(Option<i64>, Arc<dyn Process>)>,
) -> Result<(JoinHandle<Result<S>>, Arc<dyn Process>)>
where
    S: ProcessState + Send + ResourceLimiter + 'static,
{
    let id = state.id();
    trace!("Spawning process: {}", id);
    let signal_mailbox = state.signal_mailbox().clone();
    let message_mailbox = state.message_mailbox().clone();

    let instance = runtime.instantiate(module, state).await?;
    let function = function.to_string();
    let fut = async move { instance.call(&function, params).await };
    let child_process = crate::new(fut, id, env.clone(), signal_mailbox.1, message_mailbox);
    let child_process_handle = Arc::new(WasmProcess::new(id, signal_mailbox.0.clone()));

    env.add_process(id, child_process_handle.clone());

    // **Child link guarantees**:
    // The link signal is going to be put inside of the child's mailbox and is going to be
    // processed before any child code can run. This means that any failure inside the child
    // Wasm code will be correctly reported to the parent.
    //
    // We assume here that the code inside of `process::new()` will not fail during signal
    // handling.
    //
    // **Parent link guarantees**:
    // A `tokio::task::yield_now()` call is executed to allow the parent to link the child
    // before continuing any further execution. This should force the parent to process all
    // signals right away.
    //
    // The parent could have received a `kill` signal in its mailbox before this function was
    // called and this signal is going to be processed before the link is established (FIFO).
    // Only after the yield function we can guarantee that the child is going to be notified
    // if the parent fails. This is ok, as the actual spawning of the child happens after the
    // call, so the child wouldn't even exist if the parent failed before.
    //
    // TODO: The guarantees provided here don't hold anymore in a distributed environment and
    //       will require some rethinking. This function will be executed on a completely
    //       different computer and needs to be synced in a more robust way with the parent
    //       running somewhere else.
    if let Some((tag, process)) = link {
        // Send signal to itself to perform the linking
        process.send(Signal::Link(None, child_process_handle.clone()));
        // Suspend itself to process all new signals
        tokio::task::yield_now().await;
        // Send signal to child to link it
        signal_mailbox
            .0
            .send(Signal::Link(tag, process))
            .expect("receiver must exist at this point");
    }

    // Spawn a background process
    trace!("Process size: {}", std::mem::size_of_val(&child_process));
    let join = tokio::task::spawn(child_process);
    Ok((join, child_process_handle))
}
