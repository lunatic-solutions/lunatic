use std::{collections::HashMap, future::Future, hash::Hash};

use anyhow::Result;
use log::debug;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use uuid::Uuid;
use wasmtime::Val;

use crate::{mailbox::MessageMailbox, message::Message};

/// The `Process` is the main abstraction unit in lunatic.
///
/// It usually represents some code that is being executed (Wasm instance or V8 isolate), but it
/// could also be a resource (GPU, UDP connection) that can be interacted with through messages.
///
/// The only way of interacting with them is through signals. These signals can come in different
/// shapes (message, kill, link, ...). Most signals have well defined meanings, but others such as
/// a [`Message`] can be interpreted by the receiver in different ways.
pub trait Process {
    fn send(&self, signal: Signal);
}

#[derive(Debug)]
pub enum Signal {
    Message(Message),
    // When received process should stop.
    Kill,
    // Change behaviour of what happens if a linked process dies.
    DieWhenLinkDies(bool),
    // Sent from a process that wants to be linked. In case of a death the tag will be returned
    // to the sender in form of a `LinkDied` signal.
    Link(Option<i64>, WasmProcess),
    // Request from a process to be unlinked
    UnLink(WasmProcess),
    // Sent to linked processes when the link dies. Contains the tag used when the link was
    // established. Depending on the value of `die_when_link_dies` (default is `true`) this
    // receiving process will turn this signal into a message or the process will immediately
    // die as well.
    LinkDied(Option<i64>),
}

/// The reason of a process finishing
pub enum Finished<T> {
    /// The Wasm function finished or trapped
    Wasm(T),
    /// The process was terminated by an external signal
    Signal(Signal),
}

/// A `WasmProcess` represents an instance of a Wasm module that is being executed.
///
/// They are created inside the `Environment::spawn` method, and once spawned they will be running
/// in the background and can't be observed directly.
#[derive(Debug, Clone)]
pub struct WasmProcess {
    id: Uuid,
    signal_mailbox: UnboundedSender<Signal>,
}

impl PartialEq for WasmProcess {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for WasmProcess {}

impl Hash for WasmProcess {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl WasmProcess {
    /// Create a new WasmProcess
    pub fn new(id: Uuid, signal_mailbox: UnboundedSender<Signal>) -> Self {
        Self { id, signal_mailbox }
    }
}

impl Process for WasmProcess {
    fn send(&self, signal: Signal) {
        // If the receiver doesn't exist or is closed, just ignore it and drop the `signal`.
        // lunatic can't guarantee that a message was successfully seen by the receiving side even
        // if this call succeeds. We deliberately don't expose this API, as it would not make sense
        // to relay on it and could signal wrong guarantees to users.
        let _ = self.signal_mailbox.send(signal);
    }
}

// Turns a Future into a process, enabling signals (e.g. kill).
pub(crate) async fn new<F>(
    fut: F,
    mut signal_mailbox: UnboundedReceiver<Signal>,
    message_mailbox: MessageMailbox,
) where
    F: Future<Output = Result<Box<[Val]>>> + Send + 'static,
{
    // TODO: Check how big this future is. Would it make more sense to use a `Box:pin()` here?
    tokio::pin!(fut);

    // Defines what happens if one of the linked processes dies.
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
                    Some(Signal::Message(message)) => message_mailbox.push(message),
                    Some(Signal::DieWhenLinkDies(value)) => die_when_link_dies = value,
                    // Put process into list of linked processes
                    Some(Signal::Link(tag, proc)) => { links.insert(proc, tag); },
                    // Remove process from list
                    Some(Signal::UnLink(proc)) => { links.remove(&proc); }
                    // Exit loop and don't poll anymore the future if Signal::Kill received.
                    Some(Signal::Kill) => break Finished::Signal(Signal::Kill),
                    // Depending if `die_when_link_dies` is set, process will die or turn the
                    // signal into a message
                    Some(Signal::LinkDied(tag)) => {
                        if die_when_link_dies {
                            // Even this was not a **kill** signal it has the same effect on
                            // this process and should be propagated as such.
                            break Finished::Signal(Signal::Kill)
                        } else {
                            let message = Message::Signal(tag);
                            message_mailbox.push(message);
                        }
                    },
                    None => unreachable!("The process holds the sending side and is never closed")
                }
            }
            // Run process
            output = &mut fut => { break Finished::Wasm(output); }
        }
    };
    match result {
        Finished::Wasm(Result::Err(err)) => {
            debug!("Process failed: {}", err);
            // Notify all links that we finished with a trap
            links.iter().for_each(|(proc, tag)| {
                let _ = proc.send(Signal::LinkDied(*tag));
            });
        }
        Finished::Signal(Signal::Kill) => {
            debug!("Process was killed");
            // Notify all links that we finished because of a kill signal
            links.iter().for_each(|(proc, tag)| {
                let _ = proc.send(Signal::LinkDied(*tag));
            });
        }
        _ => {} // Finished normally
    }
}
