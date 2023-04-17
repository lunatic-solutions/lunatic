pub mod config;
pub mod env;
pub mod mailbox;
pub mod message;
pub mod runtimes;
pub mod state;
pub mod wasm;

use std::{collections::HashMap, fmt::Debug, future::Future, hash::Hash, sync::Arc};

use anyhow::{anyhow, Result};
use env::Environment;
use log::{debug, log_enabled, trace, warn, Level};

use lunatic_common_api::MetricsExt;
use once_cell::sync::OnceCell;
use opentelemetry::{
    global,
    metrics::{Counter, Histogram, Meter, Unit, UpDownCounter},
    KeyValue,
};
use smallvec::SmallVec;
use state::ProcessState;
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex,
    },
    task::JoinHandle,
};

use crate::{mailbox::MessageMailbox, message::Message};

pub struct SignalsMetrics {
    _meter: Meter,
    pub sent: Counter<u64>,
    pub received: Counter<u64>,
    pub link_died: Counter<u64>,
}

pub struct MessagesMetrics {
    _meter: Meter,
    pub sent: Counter<u64>,
    pub outstanding: UpDownCounter<i64>,
}

pub struct LinksMetrics {
    _meter: Meter,
    pub alive: UpDownCounter<i64>,
}

pub struct DataMessagesMetrics {
    _meter: Meter,
    pub sent: Counter<u64>,
    pub resources_count: Histogram<u64>,
    pub size: Histogram<u64>,
}

pub struct EnvironmentMetrics {
    _meter: Meter,
    pub count: UpDownCounter<i64>,
    pub process_count: UpDownCounter<i64>,
}

pub static SIGNALS_METRICS: OnceCell<SignalsMetrics> = OnceCell::new();
pub static MESSAGES_METRICS: OnceCell<MessagesMetrics> = OnceCell::new();
pub static LINKS_METRICS: OnceCell<LinksMetrics> = OnceCell::new();
pub static DATA_MESSAGES_METRICS: OnceCell<DataMessagesMetrics> = OnceCell::new();
pub static ENVIRONMENT_METRICS: OnceCell<EnvironmentMetrics> = OnceCell::new();

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
    UnLink { process_id: u64 },
    // Sent to linked processes when the link dies. Contains the tag used when the link was
    // established. Depending on the value of `die_when_link_dies` (default is `true`) and
    // the death reason, the receiving process will turn this signal into a message or the
    // process will immediately die as well.
    LinkDied(u64, Option<i64>, DeathReason),
}

impl Debug for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(_) => write!(f, "Message"),
            Self::Kill => write!(f, "Kill"),
            Self::DieWhenLinkDies(_) => write!(f, "DieWhenLinkDies"),
            Self::Link(_, p) => write!(f, "Link {}", p.id()),
            Self::UnLink { process_id } => write!(f, "UnLink {process_id}"),
            Self::LinkDied(_, _, reason) => write!(f, "LinkDied {reason:?}"),
        }
    }
}

// The reason of a process' death
#[derive(Debug)]
pub enum DeathReason {
    // Process finished normaly.
    Normal,
    Failure,
    NoProcess,
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
    signal_mailbox: UnboundedSender<Signal>,
}

impl WasmProcess {
    /// Create a new WasmProcess
    pub fn new(id: u64, signal_mailbox: UnboundedSender<Signal>) -> Self {
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
        let _ = self.signal_mailbox.send(signal);

        SIGNALS_METRICS.with_current_context(|metrics, cx| {
            metrics.sent.add(
                &cx,
                1,
                &[
                    KeyValue::new("process_kind", "wasm"),
                    KeyValue::new("process_id", self.id() as i64),
                ],
            )
        });
    }
}

/// Enum containing a process name if available, otherwise its ID.
enum NameOrID<'a> {
    Names(SmallVec<[&'a str; 2]>),
    ID(u64),
}

impl<'a> NameOrID<'a> {
    /// Returns names, otherwise id if names is empty.
    fn or_id(self, id: u64) -> Self {
        match self {
            NameOrID::Names(ref names) if !names.is_empty() => self,
            _ => NameOrID::ID(id),
        }
    }
}

impl<'a> std::fmt::Display for NameOrID<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameOrID::Names(names) => {
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        write!(f, " / ")?;
                    }
                    write!(f, "'{name}'")?;
                }
                Ok(())
            }
            NameOrID::ID(id) => write!(f, "{id}"),
        }
    }
}

impl<'a> FromIterator<&'a str> for NameOrID<'a> {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let names = SmallVec::from_iter(iter);
        NameOrID::Names(names)
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
pub(crate) async fn new<F, S, R>(
    fut: F,
    id: u64,
    env: Arc<dyn Environment>,
    signal_mailbox: Arc<Mutex<UnboundedReceiver<Signal>>>,
    message_mailbox: MessageMailbox,
) -> Result<S>
where
    S: ProcessState,
    R: Into<ExecutionResult<S>>,
    F: Future<Output = R> + Send + 'static,
{
    let result = {
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
        let mut signal_mailbox = signal_mailbox.lock().await;
        let mut has_sender = true;
        let result = loop {
            tokio::select! {
                biased;
                // Handle signals first
                signal = signal_mailbox.recv(), if has_sender => {
                    SIGNALS_METRICS.with_current_context(|metrics, cx| {
                        metrics.received.add(&cx, 1, &[KeyValue::new("process_id", id as i64)])
                    });

                    match signal.ok_or(()) {
                        Ok(Signal::Message(message)) => {
                            message.write_metrics();
                            message_mailbox.push(message);

                            MESSAGES_METRICS.with_current_context(|metrics, cx| {
                                let attrs = [KeyValue::new("process_id", id as i64)];
                                metrics.sent.add(&cx, 1, &attrs);
                                metrics.outstanding.add(&cx, 1, &attrs);
                            });
                        },
                        Ok(Signal::DieWhenLinkDies(value)) => die_when_link_dies = value,
                        // Put process into list of linked processes
                        Ok(Signal::Link(tag, proc)) => {
                            links.insert(proc.id(), (proc, tag));

                            LINKS_METRICS.with_current_context(|metrics, cx| {
                                metrics.alive.add(&cx, 1, &[KeyValue::new("process_id", id as i64)])
                            });

                        },
                        // Remove process from list
                        Ok(Signal::UnLink { process_id }) => {
                            links.remove(&process_id);

                            LINKS_METRICS.with_current_context(|metrics, cx| {
                                metrics.alive.add(&cx, -1, &[KeyValue::new("process_id", id as i64)])
                            });
                        }
                        // Exit loop and don't poll anymore the future if Signal::Kill received.
                        Ok(Signal::Kill) => break Finished::KillSignal,
                        // Depending if `die_when_link_dies` is set, process will die or turn the
                        // signal into a message
                        Ok(Signal::LinkDied(id, tag, reason)) => {
                            links.remove(&id);

                            LINKS_METRICS.with_current_context(|metrics, cx| {
                                metrics.alive.add(&cx, -1, &[KeyValue::new("process_id", id as i64)])
                            });

                            match reason {
                                DeathReason::Failure | DeathReason::NoProcess => {
                                    if die_when_link_dies {
                                        // Even this was not a **kill** signal it has the same effect on
                                        // this process and should be propagated as such.
                                        break Finished::KillSignal;
                                    } else {
                                        let message = Message::LinkDied(tag);
                                        message_mailbox.push(message);

                                        MESSAGES_METRICS.with_current_context(|metrics, cx| {
                                            metrics.outstanding.add(&cx, 1, &[KeyValue::new("process_id", id as i64)]);
                                        });

                                    }
                                },
                                // In case a linked process finishes normally, don't do anything.
                                DeathReason::Normal => {},
                            }
                        },
                        Err(_) => {
                            debug_assert!(has_sender);
                            has_sender = false;
                        }
                    }
                }
                // Run process
                output = &mut fut => { break Finished::Normal(output); }
            }
        };

        MESSAGES_METRICS.with_current_context(|metrics, cx| {
            metrics.outstanding.add(
                &cx,
                -(message_mailbox.len() as i64),
                &[KeyValue::new("process_id", id as i64)],
            );
        });

        match result {
            Finished::Normal(result) => {
                let result: ExecutionResult<_> = result.into();

                if let Some(failure) = result.failure() {
                    let registry = result.state().registry().read().await;
                    let name = registry
                        .iter()
                        .filter(|(_, (_, process_id))| process_id == &id)
                        .map(|(name, _)| name.splitn(4, '/').last().unwrap_or(name.as_str()))
                        .collect::<NameOrID>()
                        .or_id(id);
                    warn!(
                        "Process {} failed, notifying: {} links {}",
                        name,
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
                    links.iter().for_each(|(_, (proc, tag))| {
                        proc.send(Signal::LinkDied(id, *tag, DeathReason::Failure));
                    });
                    Err(anyhow!(failure.to_string()))
                } else {
                    // Notify all links that we finished normally
                    links.iter().for_each(|(_, (proc, tag))| {
                        proc.send(Signal::LinkDied(id, *tag, DeathReason::Normal));
                    });
                    Ok(result.into_state())
                }
            }
            Finished::KillSignal => {
                warn!(
                    "Process {} was killed, notifying: {} links",
                    id,
                    links.len()
                );

                // Notify all links that we finished because of a kill signal
                links.iter().for_each(|(_, (proc, tag))| {
                    proc.send(Signal::LinkDied(id, *tag, DeathReason::Failure));
                });

                Err(anyhow!("Process received Kill signal"))
            }
        }
    };

    env.remove_process(id);

    result
}

/// A process spawned from a native Rust closure.
#[derive(Clone, Debug)]
pub struct NativeProcess {
    id: u64,
    signal_mailbox: UnboundedSender<Signal>,
}

/// Spawns a process from a closure.
pub fn spawn<T, F, K, R>(
    env: Arc<dyn Environment>,
    func: F,
) -> (JoinHandle<Result<T>>, NativeProcess)
where
    T: ProcessState + Send + Sync + 'static,
    R: Into<ExecutionResult<T>> + Send + 'static,
    K: Future<Output = R> + Send + 'static,
    F: FnOnce(NativeProcess, MessageMailbox) -> K,
{
    let id = env.get_next_process_id();
    let (signal_sender, signal_mailbox) = unbounded_channel::<Signal>();
    let message_mailbox = MessageMailbox::default();
    let process = NativeProcess {
        id,
        signal_mailbox: signal_sender,
    };
    let fut = func(process.clone(), message_mailbox.clone());
    let signal_mailbox = Arc::new(Mutex::new(signal_mailbox));
    let join = tokio::task::spawn(new(fut, id, env.clone(), signal_mailbox, message_mailbox));
    (join, process)
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
        let _ = self.signal_mailbox.send(signal);

        SIGNALS_METRICS.with_current_context(|metrics, cx| {
            metrics.sent.add(
                &cx,
                1,
                &[
                    KeyValue::new("process_kind", "native"),
                    KeyValue::new("process_id", self.id() as i64),
                ],
            );
        });
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

    // Returns the process state reference
    pub fn state(&self) -> &T {
        &self.state
    }

    // Returns the process state
    pub fn into_state(self) -> T {
        self.state
    }
}

// It's more convinient to return a `Result<T,E>` in a `NativeProcess`.
impl<T> From<Result<T>> for ExecutionResult<T>
where
    T: Default,
{
    fn from(result: Result<T>) -> Self {
        match result {
            Ok(t) => ExecutionResult {
                state: t,
                result: ResultValue::Ok,
            },
            Err(e) => ExecutionResult {
                state: T::default(),
                result: ResultValue::Failed(e.to_string()),
            },
        }
    }
}

pub enum ResultValue {
    Ok,
    Failed(String),
    SpawnError(String),
    Killed,
}
pub fn init_metrics() {
    SIGNALS_METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.process.signals");

        let sent = meter
            .u64_counter("sent")
            .with_unit(Unit::new("count"))
            .with_description("Number of signals sent to processes since startup")
            .init();
        let received = meter
            .u64_counter("received")
            .with_unit(Unit::new("count"))
            .with_description("Number of signals received by processes since startup")
            .init();
        let link_died = meter
            .u64_counter("link_died")
            .with_unit(Unit::new("count"))
            .with_description("Number of LinkDied messages send since startup")
            .init();

        SignalsMetrics {
            _meter: meter,
            sent,
            received,
            link_died,
        }
    });

    MESSAGES_METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.process.messages");

        let sent = meter
            .u64_counter("sent")
            .with_unit(Unit::new("count"))
            .with_description("Number of messages sent to processes since startup")
            .init();
        let outstanding = meter
            .i64_up_down_counter("outstanding")
            .with_unit(Unit::new("count"))
            .with_description(
                "Current number of messages that are ready to be consumed by processes",
            )
            .init();

        MessagesMetrics {
            _meter: meter,
            sent,
            outstanding,
        }
    });

    LINKS_METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.process.links");

        let alive = meter
            .i64_up_down_counter("alive")
            .with_unit(Unit::new("count"))
            .with_description("Number of links currently alive")
            .init();

        LinksMetrics {
            _meter: meter,
            alive,
        }
    });

    DATA_MESSAGES_METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.process.messages.data");

        let sent = meter
            .u64_counter("sent")
            .with_unit(Unit::new("count"))
            .with_description("Number of data messages sent since startup")
            .init();
        let resources_count = meter
            .u64_histogram("resources_count")
            .with_unit(Unit::new("count"))
            .with_description("Number of resources used by each individual message")
            .init();
        let size = meter
            .u64_histogram("size")
            .with_unit(Unit::new("bytes"))
            .with_description("Number of bytes used by each individual process")
            .init();

        DataMessagesMetrics {
            _meter: meter,
            sent,
            resources_count,
            size,
        }
    });

    ENVIRONMENT_METRICS.get_or_init(|| {
        let meter = global::meter("lunatic.environment");

        let count = meter
            .i64_up_down_counter("count")
            .with_unit(Unit::new("count"))
            .with_description("Number of currently active environments")
            .init();
        let process_count = meter
            .i64_up_down_counter("process_count")
            .with_unit(Unit::new("count"))
            .with_description("Number of currently registered processes")
            .init();

        EnvironmentMetrics {
            _meter: meter,
            count,
            process_count,
        }
    });
}
