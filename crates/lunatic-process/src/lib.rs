pub mod config;
pub mod env;
pub mod local_control;
pub mod local_dist;
pub mod mailbox;
pub mod message;
pub mod runtimes;
pub mod state;
pub mod wasm;

use std::{collections::HashMap, fmt::Debug, future::Future, hash::Hash, sync::Arc};

use anyhow::{anyhow, Result};
use env::Environment;
use log::{debug, log_enabled, trace, warn, Level};

use async_std::channel::{unbounded, Receiver, Sender};
use async_std::task::JoinHandle;

use crate::{mailbox::MessageMailbox, message::Message};

/// The `Process` is the main abstraction in lunatic.
///
/// It usually represents some code that is being executed (Wasm instance or V8 isolate), but it
/// could also be a resource (GPU, UDP connection) that can be interacted with through messages.
///
/// The only way of interacting with them is through signals. These signals can come in different
/// shapes (message, kill, link, ...). Most signals have well defined meanings, but others such as
/// a [`Message`] are opaque and left to the receiver for interpretation.
pub trait Process: Send + Sync {
    fn id(&self) -> u64;
    fn send(&self, signal: Signal);
}

impl Debug for dyn Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Point").field("id", &self.id()).finish()
    }
}

impl PartialEq<dyn Process> for dyn Process {
    fn eq(&self, other: &dyn Process) -> bool {
        self.id() == other.id()
    }
}

impl Eq for dyn Process {}

impl Hash for dyn Process {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

/// Signals can be sent to processes to interact with them.
pub enum Signal {
    // Messages can contain opaque data.
    Message(Message),
    // When received, the process should stop immediately.
    Kill,
    // Change behaviour of what happens if a linked process dies.
    DieWhenLinkDies(bool),
    // Sent from a process that wants to be linked. In case of a death the tag will be returned
    // to the sender in form of a `LinkDied` signal.
    Link(Option<i64>, Arc<dyn Process>),
    // Request from a process to be unlinked
    UnLink(Arc<dyn Process>),
    // Sent to linked processes when the link dies. Contains the tag used when the link was
    // established. Depending on the value of `die_when_link_dies` (default is `true`) this
    // receiving process will turn this signal into a message or the process will immediately
    // die as well.
    LinkDied(Option<i64>),
}

impl Debug for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(_) => write!(f, "Message"),
            Self::Kill => write!(f, "Kill"),
            Self::DieWhenLinkDies(_) => write!(f, "DieWhenLinkDies"),
            Self::Link(_, _) => write!(f, "Link"),
            Self::UnLink(_) => write!(f, "UnLink"),
            Self::LinkDied(_) => write!(f, "LinkDied"),
        }
    }
}

/// The reason of a process finishing
pub enum Finished<T> {
    /// This just means that the process finished without external interaction.
    /// In case of Wasm this could mean that the entry function returned normally or that it
    /// **trapped**.
    Normal(T),
    /// The process was terminated by an external `Kill` signal.
    KillSignal,
}

/// A `WasmProcess` represents an instance of a Wasm module that is being executed.
///
/// They can be created with [`spawn_wasm`](crate::wasm::spawn_wasm), and once spawned they will be
/// running in the background and can't be observed directly.
#[derive(Debug, Clone)]
pub struct WasmProcess {
    id: u64,
    signal_mailbox: Sender<Signal>,
}

impl WasmProcess {
    /// Create a new WasmProcess
    pub fn new(id: u64, signal_mailbox: Sender<Signal>) -> Self {
        Self { id, signal_mailbox }
    }
}

impl Process for WasmProcess {
    fn id(&self) -> u64 {
        self.id
    }
    fn send(&self, signal: Signal) {
        // If the receiver doesn't exist or is closed, just ignore it and drop the `signal`.
        // lunatic can't guarantee that a message was successfully seen by the receiving side even
        // if this call succeeds. We deliberately don't expose this API, as it would not make sense
        // to relay on it and could signal wrong guarantees to users.
        let _ = self.signal_mailbox.try_send(signal);
    }
}

/// Turns a `Future` into a process, enabling signals (e.g. kill).
///
/// This function represents the core execution loop of lunatic processes:
///
/// 1. The process will first check if there are any new signals and handle them.
/// 2. If no signals are available, it will poll the `Future` and advance the execution.
///
/// This steps are repeated until the `Future` returns `Poll::Ready`, indicating the end of the
/// computation.
///
/// The `Future` is in charge to periodically yield back the execution with `Poll::Pending` to give
/// the signal handler a chance to run and process pending signals.
///
/// In case of success, the process state `S` is returned. It's not possible to return the process
/// state in case of failure because of limitations in the Wasmtime API:
/// https://github.com/bytecodealliance/wasmtime/issues/2986
pub(crate) async fn new<F, S>(
    fut: F,
    id: u64,
    signal_mailbox: Receiver<Signal>,
    message_mailbox: MessageMailbox,
) -> Result<S>
where
    F: Future<Output = ExecutionResult<S>> + Send + 'static,
{
    trace!("Process {} spawned", id);
    tokio::pin!(fut);

    // Defines what happens if one of the linked processes dies.
    // If the value is set to false, instead of dying too the process will receive a message about
    // the linked process' death.
    let mut die_when_link_dies = true;
    // Process linked to this one
    let mut links = HashMap::new();
    // TODO: Maybe wrapping this in some kind of `std::panic::catch_unwind` wold be a good idea,
    //       to protect against panics in host function calls that unwind through Wasm code.
    //       Currently a panic would just kill the task, but not notify linked processes.
    let result = loop {
        tokio::select! {
            biased;
            // Handle signals first
            signal = signal_mailbox.recv() => {
                match signal {
                    Ok(Signal::Message(message)) => message_mailbox.push(message),
                    Ok(Signal::DieWhenLinkDies(value)) => die_when_link_dies = value,
                    // Put process into list of linked processes
                    Ok(Signal::Link(tag, proc)) => { links.insert(proc, tag); },
                    // Remove process from list
                    Ok(Signal::UnLink(proc)) => { links.remove(&proc); }
                    // Exit loop and don't poll anymore the future if Signal::Kill received.
                    Ok(Signal::Kill) => break Finished::KillSignal,
                    // Depending if `die_when_link_dies` is set, process will die or turn the
                    // signal into a message
                    Ok(Signal::LinkDied(tag)) => {
                        if die_when_link_dies {
                            // Even this was not a **kill** signal it has the same effect on
                            // this process and should be propagated as such.
                            // TODO: Remove sender from our notify list, so we don't send back the
                            //       same notification to an already dead process.
                            break Finished::KillSignal
                        } else {
                            let message = Message::LinkDied(tag);
                            message_mailbox.push(message);
                        }
                    },
                    Err(_) => unreachable!("The process holds the sending side and is not closed")
                }
            }
            // Run process
            output = &mut fut => { break Finished::Normal(output); }
        }
    };
    match result {
        Finished::Normal(result) => {
            if let Some(failure) = result.failure() {
                warn!(
                    "Process {} failed, notifying: {} links {}",
                    id,
                    links.len(),
                    // If the log level is WARN instruct user how to display the stacktrace
                    if !log_enabled!(Level::Debug) {
                        "\n\t\t\t    (Set ENV variable `RUST_LOG=lunatic=debug` to show stacktrace)"
                    } else {
                        ""
                    }
                );
                debug!("{}", failure);
                // Notify all links that we finished with an error
                links.iter().for_each(|(proc, tag)| {
                    let _ = proc.send(Signal::LinkDied(*tag));
                });
                Err(anyhow!(failure.to_string()))
            } else {
                Ok(result.state())
            }
        }
        Finished::KillSignal => {
            warn!(
                "Process {} was killed, notifying: {} links",
                id,
                links.len()
            );
            // Notify all links that we finished because of a kill signal
            links.iter().for_each(|(proc, tag)| {
                let _ = proc.send(Signal::LinkDied(*tag));
            });
            Err(anyhow!("Process received Kill signal"))
        }
    }
}

/// A process spawned from a native Rust closure.
#[derive(Clone, Debug)]
pub struct NativeProcess {
    id: u64,
    signal_mailbox: Sender<Signal>,
}

/// Spawns a process from a closure.
///
/// ## Example:
///
/// ```no_run
/// let _proc = lunatic_runtime::spawn(|_this, mailbox| async move {
///     // Wait on a message with the tag `27`.
///     mailbox.pop(Some(&[27])).await;
///     Ok(())
/// });
/// ```

impl Environment {
    pub fn spawn<T, F, K>(&self, func: F) -> (JoinHandle<Result<T>>, NativeProcess)
    where
        T: Send + 'static,
        K: Future<Output = ExecutionResult<T>> + Send + 'static,
        F: FnOnce(NativeProcess, MessageMailbox) -> K,
    {
        let id = self.get_next_process_id();
        let (signal_sender, signal_mailbox) = unbounded::<Signal>();
        let message_mailbox = MessageMailbox::default();
        let process = NativeProcess {
            id,
            signal_mailbox: signal_sender,
        };
        let fut = func(process.clone(), message_mailbox.clone());
        let join = async_std::task::spawn(new(fut, id, signal_mailbox, message_mailbox));
        (join, process)
    }
}

impl Process for NativeProcess {
    fn id(&self) -> u64 {
        self.id
    }
    fn send(&self, signal: Signal) {
        // If the receiver doesn't exist or is closed, just ignore it and drop the `signal`.
        // lunatic can't guarantee that a message was successfully seen by the receiving side even
        // if this call succeeds. We deliberately don't expose this API, as it would not make sense
        // to relay on it and could signal wrong guarantees to users.
        let _ = self.signal_mailbox.try_send(signal);
    }
}

// Contains the result of a process execution.
//
// Can be also used to extract the state of a process after the execution is done.
pub struct ExecutionResult<T> {
    state: T,
    result: ResultValue,
}

impl<T> ExecutionResult<T> {
    // Returns the failure as `String` if the process failed.
    pub fn failure(&self) -> Option<&str> {
        match self.result {
            ResultValue::Failed(ref failure) => Some(failure),
            ResultValue::SpawnError(ref failure) => Some(failure),
            _ => None,
        }
    }

    // Returns the process state
    pub fn state(self) -> T {
        self.state
    }
}

#[derive(PartialEq, Eq)]
pub enum ResultValue {
    Ok,
    Failed(String),
    SpawnError(String),
}
