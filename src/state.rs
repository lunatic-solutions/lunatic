use std::any::type_name;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::vec::IntoIter;

use anyhow::Result;
use async_std::channel::Sender;
use async_std::net::{TcpListener, TcpStream};
use uuid::Uuid;
use wasmtime::{ResourceLimiter, Trap};
use wasmtime_wasi::{ambient_authority, Dir, WasiCtx, WasiCtxBuilder};

use crate::mailbox::MessageMailbox;
use crate::module::Module;
use crate::plugin::ModuleContext;
use crate::{message::Message, EnvConfig, Environment};
use crate::{Process, Signal};

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
    // The module that this process was spawned from
    pub(crate) module: Module,
    // A message currently being written.
    pub(crate) draft: Option<Message>,
    // A message currently being read. Host functions operate on this message after it is received.
    // For example, a guest might want to read message bytes, get reply handle, etc. 
    pub(crate) reading: Option<Message>,
    // Message id local to the sending process. It's used for reply ids.
    last_message_id: NonZeroU64,
    // Reply id handles. This makes message and reply ids opaque for the guest code.
    pub(crate) reply_ids: HashMapId<NonZeroU64>,
    // This field is only part of the state to make it possible to create a Wasm process handle
    // from inside itself. See the `lunatic::process::this()` Wasm API.
    pub(crate) signal_mailbox: Sender<Signal>,
    // Messages sent to the process
    pub(crate) message_mailbox: MessageMailbox,
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
        module: Module,
        signal_mailbox: Sender<Signal>,
        message_mailbox: MessageMailbox,
        config: &EnvConfig,
    ) -> Result<Self> {
        let mut wasi = WasiCtxBuilder::new().inherit_stdio();
        if let Some(envs) = config.wasi_envs() {
            wasi = wasi.envs(envs)?;
        }
        if let Some(args) = config.wasi_args() {
            wasi = wasi.args(args)?;
        }
        for preopen_dir_path in config.preopened_dirs() {
            let preopen_dir = Dir::open_ambient_dir(preopen_dir_path, ambient_authority())?;
            wasi = wasi.preopened_dir(preopen_dir, preopen_dir_path)?;
        }
        let state = Self {
            id,
            module,
            draft: None,
            reading: None,
            last_message_id: NonZeroU64::new(1).unwrap(),
            reply_ids: HashMapId::new(),
            signal_mailbox,
            message_mailbox,
            errors: HashMapId::new(),
            resources: Resources::default(),
            wasi: wasi.build(),
        };
        Ok(state)
    }

    pub fn generate_message_id(&mut self) -> Result<NonZeroU64, Trap> {
        self.last_message_id = NonZeroU64::new(self.last_message_id.get() + 1)
            .ok_or_else(|| Trap::new("Last message id overflowed"))?;
        Ok(self.last_message_id)
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
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> bool {
        desired <= self.module.environment().config().max_memory()
    }

    // TODO: What would be a reasonable table limit be?
    fn table_growing(&mut self, _current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        desired < 10_000
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
    pub(crate) processes: HashMapId<Arc<dyn Process>>,
    pub(crate) dns_iterators: HashMapId<DnsIterator>,
    pub(crate) tcp_listeners: HashMapId<TcpListener>,
    pub(crate) tcp_streams: HashMapId<TcpStream>,
}

/// HashMap wrapper with incremental ID (u64) assignment.
pub(crate) struct HashMapId<T> {
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
