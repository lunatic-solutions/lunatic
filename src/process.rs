use std::num::NonZeroU64;
use std::{collections::HashMap, fmt::Debug, future::Future, hash::Hash, sync::Arc};

use anyhow::Result;
use log::{debug, trace};

use async_std::channel::{unbounded, Receiver, Sender};
use async_std::task::JoinHandle;

use uuid::Uuid;

use crate::{mailbox::MessageMailbox, message::Message};

// TODO: Consider switching to different type id, maybe a custom or uuid v1?
#[derive(Debug, PartialEq, Hash, Clone, Copy)]
pub struct ProcessId(Uuid);

impl ProcessId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        ProcessId(Uuid::new_v4())
    }

    pub fn as_u128(&self) -> u128 {
        self.0.as_u128()
    }
}

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The `Process` is the main abstraction unit in lunatic.
///
/// It usually represents some code that is being executed (Wasm instance or V8 isolate), but it
/// could also be a resource (GPU, UDP connection) that can be interacted with through messages.
///
/// The only way of interacting with them is through signals. These signals can come in different
/// shapes (message, kill, link, ...). Most signals have well defined meanings, but others such as
/// a [`Message`] can be interpreted by the receiver in different ways.
pub trait Process: Send + Sync {
    fn id(&self) -> ProcessId;
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
    Message(Message),
    // When received process should stop.
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
    /// The process was terminated by an external signal.
    Signal(Signal),
}

/// A `WasmProcess` represents an instance of a Wasm module that is being executed.
///
/// They are created inside the `Environment::spawn` method, and once spawned they will be running
/// in the background and can't be observed directly.
#[derive(Debug, Clone)]
pub struct WasmProcess {
    id: ProcessId,
    signal_mailbox: Sender<Signal>,
}

impl WasmProcess {
    /// Create a new WasmProcess
    pub fn new(id: ProcessId, signal_mailbox: Sender<Signal>) -> Self {
        Self { id, signal_mailbox }
    }
}

impl Process for WasmProcess {
    fn id(&self) -> ProcessId {
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

// Turns a Future into a process, enabling signals (e.g. kill).
pub(crate) async fn new<F, T>(
    fut: F,
    id: ProcessId,
    signal_mailbox: Receiver<Signal>,
    message_mailbox: MessageMailbox,
) where
    F: Future<Output = Result<T>> + Send + 'static,
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
                    Ok(Signal::Kill) => break Finished::Signal(Signal::Kill),
                    // Depending if `die_when_link_dies` is set, process will die or turn the
                    // signal into a message
                    Ok(Signal::LinkDied(tag)) => {
                        if die_when_link_dies {
                            // Even this was not a **kill** signal it has the same effect on
                            // this process and should be propagated as such.
                            break Finished::Signal(Signal::Kill)
                        } else {
                            // TODO message id?
                            let message = Message::new_signal(NonZeroU64::new(1).unwrap(), id, tag);
                            message_mailbox.push(message);
                        }
                    },
                    Err(_) => unreachable!("The process holds the sending side and is never closed")
                }
            }
            // Run process
            output = &mut fut => { break Finished::Normal(output); }
        }
    };
    match result {
        Finished::Normal(Result::Err(err)) => {
            // If the trap is a result of calling `proc_exit(0)` treat it as an no-error finish.
            if let Some(trap) = err.downcast_ref::<wasmtime::Trap>() {
                if let Some(exit_status) = trap.i32_exit_status() {
                    if exit_status == 0 {
                        return;
                    }
                }
            };
            debug!("Process {} failed: {}", id, err);
            // Notify all links that we finished with an error
            links.iter().for_each(|(proc, tag)| {
                let _ = proc.send(Signal::LinkDied(*tag));
            });
        }
        Finished::Signal(Signal::Kill) => {
            debug!("Process {} was killed", id);
            // Notify all links that we finished because of a kill signal
            links.iter().for_each(|(proc, tag)| {
                let _ = proc.send(Signal::LinkDied(*tag));
            });
        }
        _ => {} // Finished normally
    }
}

/// A process spawned from a native Rust closure.
#[derive(Clone, Debug)]
pub struct NativeProcess {
    id: ProcessId,
    signal_mailbox: Sender<Signal>,
}

/// Spawns a process from a closure.
///
/// ## Example:
///
/// ```no_run
/// let _proc = lunatic_runtime::spawn(|mailbox| async move {
///     // Wait on a message with the tag `27`.
///     mailbox.pop(Some(&[27])).await;
///     Ok(())
/// });
/// ```
pub fn spawn<F, K, T>(func: F) -> (JoinHandle<()>, NativeProcess)
where
    T: 'static,
    K: Future<Output = Result<T>> + Send + 'static,
    F: Fn(MessageMailbox) -> K,
{
    let id = ProcessId::new();
    let (signal_sender, signal_mailbox) = unbounded::<Signal>();
    let message_mailbox = MessageMailbox::default();
    let process = NativeProcess {
        id,
        signal_mailbox: signal_sender,
    };
    let fut = func(message_mailbox.clone());
    let join = async_std::task::spawn(new(fut, id, signal_mailbox, message_mailbox));
    (join, process)
}

impl Process for NativeProcess {
    fn id(&self) -> ProcessId {
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
