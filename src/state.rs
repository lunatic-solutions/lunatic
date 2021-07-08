use std::any::type_name;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;
use std::vec::IntoIter;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};
use wasmtime::{Module, ResourceLimiter};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

use crate::environment::EnvInfo;
use crate::{message::Message, process::ProcessHandle, EnvConfig, Environment};

// The internal state of each Process.
//
// Host functions will share one state.
pub(crate) struct State {
    // Information about the environment that this process was spawned from
    pub(crate) env_info: EnvInfo,
    // A space that can be used to temporarily store messages when sending or receiving them.
    // Messages can contain resources that need to be added across multiple host. Likewise,
    // receiving messages is done in two steps, first the message size is returned to allow the
    // guest to reserve enough space and then the it's received. Both of those actions use
    // `message` as a temp space to store messages across host calls.
    pub(crate) message: Option<Message>,
    // Messages sent to the process
    pub(crate) mailbox: UnboundedReceiver<Message>,
    // Errors belonging to the process
    pub(crate) errors: HashMapId<anyhow::Error>,
    // Resources
    pub(crate) resources: Resources,
    // WASI
    pub(crate) wasi: WasiCtx,
    // The module that is being added to the environment.
    // This makes it accessible inside of plugins that run on the module before it's compiled.
    pub(crate) module_loaded: Option<Vec<u8>>,
}

impl State {
    pub fn new(env_info: EnvInfo, mailbox: UnboundedReceiver<Message>) -> Self {
        Self {
            env_info,
            message: None,
            mailbox,
            errors: HashMapId::new(),
            resources: Resources::default(),
            // TODO: Inherit args & envs
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            module_loaded: None,
        }
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("process", &self.resources)
            .finish()
    }
}

// Only allow as many instances, tables and memories as their are plugins + 1 for the process.
// Limit the maximum memory of the process depending on the environment it was spawned in.
impl ResourceLimiter for State {
    fn memory_growing(&mut self, current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        const WASM_PAGE: u64 = 64 * 1024; // bytes
        if (current as u64 + desired as u64) * WASM_PAGE < self.env_info.max_memory {
            true
        } else {
            false
        }
    }

    // TODO: What would be a reasonable table limit be?
    fn table_growing(&mut self, current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        if (current as u64 + desired as u64) < 10_000 {
            true
        } else {
            false
        }
    }

    fn instances(&self) -> usize {
        self.env_info.plugin_count + 1
    }

    fn tables(&self) -> usize {
        self.env_info.plugin_count + 1
    }

    fn memories(&self) -> usize {
        self.env_info.plugin_count + 1
    }
}

#[derive(Default, Debug)]
pub(crate) struct Resources {
    pub(crate) configs: HashMapId<EnvConfig>,
    pub(crate) environments: HashMapId<Environment>,
    pub(crate) modules: HashMapId<Module>,
    pub(crate) processes: HashMapId<ProcessHandle>,
    pub(crate) dns_iterators: HashMapId<DnsIterator>,
    pub(crate) socket_addresses: HashMapId<SocketAddr>,
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
