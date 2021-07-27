use std::any::type_name;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;
use std::vec::IntoIter;

use anyhow::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};
use uuid::Uuid;
use wasmtime::ResourceLimiter;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

use crate::module::Module;
use crate::plugin::ModuleContext;
use crate::process::Signal;
use crate::{message::Message, process::ProcessHandle, EnvConfig, Environment};

// The internal state of Plugins.
pub(crate) struct PluginState<'a, 'b> {
    // Errors belonging to the plugin
    pub(crate) errors: HashMapId<anyhow::Error>,
    module_context: &'a mut ModuleContext<'b>,
}

impl<'a, 'b> PluginState<'a, 'b> {
    pub fn new(module_context: &'a mut ModuleContext<'b>) -> Self {
        Self {
            errors: HashMapId::new(),
            module_context,
        }
    }

    pub fn module_context(&mut self) -> &mut ModuleContext<'b> {
        &mut self.module_context
    }
}

// The internal state of each Process.
//
// Host functions will share one state.
pub(crate) struct ProcessState {
    // Process id
    pub(crate) id: Uuid,
    // This fields allows us to wait on the process to finish and check if it trapped during the
    // execution, received a Signal::Kill or finished regularly.
    pub(crate) trapped_sender: Sender<bool>,
    // The module that this process was spawned from
    pub(crate) module: Module,
    // A space that can be used to temporarily store messages when sending or receiving them.
    // Messages can contain resources that need to be added across multiple host. Likewise,
    // receiving messages is done in two steps, first the message size is returned to allow the
    // guest to reserve enough space and then the it's received. Both of those actions use
    // `message` as a temp space to store messages across host calls.
    pub(crate) message: Option<Message>,
    // Message sender
    pub(crate) message_sender: UnboundedSender<Message>,
    // Signal sender
    pub(crate) signal_sender: UnboundedSender<Signal>,
    // Messages sent to the process
    pub(crate) mailbox: UnboundedReceiver<Message>,
    // Errors belonging to the process
    pub(crate) errors: HashMapId<anyhow::Error>,
    // Resources
    pub(crate) resources: Resources,
    // WASI
    pub(crate) wasi: WasiCtx,
}

impl ProcessState {
    pub fn new(
        id: Uuid,
        trapped_sender: Sender<bool>,
        module: Module,
        message_sender: UnboundedSender<Message>,
        mailbox: UnboundedReceiver<Message>,
        signal_sender: UnboundedSender<Signal>,
    ) -> Result<Self> {
        let wasi = WasiCtxBuilder::new();
        let wasi = wasi.inherit_stdio();
        let wasi = wasi.inherit_env()?;
        // Skip the first argument (`lunatic`) from the args list. When compiling existing
        // applications to Wasm, they assume that the first argument is going to be the binary
        // name and all other arguments follow.
        let args: Vec<String> = std::env::args().skip(1).collect();
        let wasi = wasi.args(&args)?;
        let state = Self {
            id,
            trapped_sender,
            module,
            message: None,
            message_sender,
            signal_sender,
            mailbox,
            errors: HashMapId::new(),
            resources: Resources::default(),
            wasi: wasi.build(),
        };
        Ok(state)
    }
}

impl Debug for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("process", &self.resources)
            .finish()
    }
}

// Limit the maximum memory of the process depending on the environment it was spawned in.
impl ResourceLimiter for ProcessState {
    fn memory_growing(&mut self, current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        const WASM_PAGE: u64 = 64 * 1024; // bytes
        (current as u64 + desired as u64) * WASM_PAGE
            < self.module.environment().config().max_memory()
    }

    // TODO: What would be a reasonable table limit be?
    fn table_growing(&mut self, current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        (current as u64 + desired as u64) < 10_000
    }

    // Allow one instance per store
    fn instances(&self) -> usize {
        1
    }

    // Allow one table per store
    fn tables(&self) -> usize {
        1
    }

    // Allow one memory per store
    fn memories(&self) -> usize {
        1
    }
}

#[derive(Default, Debug)]
pub(crate) struct Resources {
    pub(crate) configs: HashMapId<EnvConfig>,
    pub(crate) environments: HashMapId<Environment>,
    pub(crate) modules: HashMapId<Module>,
    pub(crate) processes: HashMapId<ProcessHandle>,
    pub(crate) dns_iterators: HashMapId<DnsIterator>,
    pub(crate) tcp_listeners: HashMapId<TcpListener>,
    pub(crate) tcp_streams: HashMapId<Arc<Mutex<TcpStream>>>,
}

/// HashMap wrapper with incremental ID (u64) assignment.
pub struct HashMapId<T> {
    id_seed: u64,
    store: HashMap<u64, T>,
}

impl<T> HashMapId<T>
where
    T: Send + Sync,
{
    pub fn new() -> Self {
        Self {
            id_seed: 0,
            store: HashMap::new(),
        }
    }

    pub fn add(&mut self, item: T) -> u64 {
        let id = self.id_seed;
        self.store.insert(id, item);
        self.id_seed += 1;
        id
    }

    pub fn remove(&mut self, id: u64) -> Option<T> {
        self.store.remove(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.store.get_mut(&id)
    }

    pub fn get(&self, id: u64) -> Option<&T> {
        self.store.get(&id)
    }
}

impl<T> Default for HashMapId<T>
where
    T: Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Debug for HashMapId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashMapId")
            .field("id_seed", &self.id_seed)
            .field("type", &type_name::<T>())
            .finish()
    }
}

pub(crate) struct DnsIterator {
    iter: IntoIter<SocketAddr>,
}

impl DnsIterator {
    pub fn new(iter: IntoIter<SocketAddr>) -> Self {
        Self { iter }
    }
}

impl Iterator for DnsIterator {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
