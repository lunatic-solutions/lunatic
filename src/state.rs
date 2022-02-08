use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use async_std::channel::Sender;
use async_std::net::{TcpListener, TcpStream, UdpSocket};
use hash_map_id::HashMapId;
use lunatic_error_api::{ErrorCtx, ErrorResource};
use lunatic_messaging_api::ProcessCtx;
use lunatic_networking_api::dns::DnsIterator;
use lunatic_networking_api::NetworkingCtx;
use lunatic_process::{mailbox::MessageMailbox, message::Message, Process, Signal};
use uuid::Uuid;
use wasmtime::ResourceLimiter;
use wasmtime_wasi::{ambient_authority, Dir, WasiCtx, WasiCtxBuilder};

use crate::module::Module;
use crate::{EnvConfig, Environment};

// The internal state of each Process.
//
// Host functions will share one state.
pub(crate) struct ProcessState {
    // Process id
    pub(crate) id: Uuid,
    // The module that this process was spawned from
    pub(crate) module: Module,
    // A space that can be used to temporarily store messages when sending or receiving them.
    // Messages can contain resources that need to be added across multiple host. Likewise,
    // receiving messages is done in two steps, first the message size is returned to allow the
    // guest to reserve enough space and then the it's received. Both of those actions use
    // `message` as a temp space to store messages across host calls.
    pub(crate) message: Option<Message>,
    // This field is only part of the state to make it possible to create a Wasm process handle
    // from inside itself. See the `lunatic::process::this()` Wasm API.
    pub(crate) signal_mailbox: Sender<Signal>,
    // Messages sent to the process
    pub(crate) message_mailbox: MessageMailbox,
    // Resources
    pub(crate) resources: Resources,
    // WASI
    pub(crate) wasi: WasiCtx,
    // Set to true if the WASM module has been instantiated
    pub(crate) initialized: bool,
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
            message: None,
            signal_mailbox,
            message_mailbox,
            resources: Resources::default(),
            wasi: wasi.build(),
            initialized: false,
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

impl ErrorCtx for ProcessState {
    fn error_resources(&self) -> &ErrorResource {
        &self.resources.errors
    }

    fn error_resources_mut(&mut self) -> &mut ErrorResource {
        &mut self.resources.errors
    }
}

impl ProcessCtx for ProcessState {
    fn mailbox(&mut self) -> &mut MessageMailbox {
        &mut self.message_mailbox
    }

    fn message_scratch_area(&mut self) -> &mut Option<Message> {
        &mut self.message
    }

    fn process_resources_mut(&mut self) -> &mut lunatic_messaging_api::ProcessResource {
        &mut self.resources.processes
    }

    fn tcp_resources_mut(&mut self) -> &mut lunatic_messaging_api::TcpResource {
        &mut self.resources.tcp_streams
    }

    fn udp_resources_mut(&mut self) -> &mut lunatic_messaging_api::UdpResource {
        &mut self.resources.udp_sockets
    }
}

impl NetworkingCtx for ProcessState {
    fn tcp_listener_resources(&self) -> &lunatic_networking_api::TcpListenerResources {
        &self.resources.tcp_listeners
    }

    fn tcp_listener_resources_mut(&mut self) -> &mut lunatic_networking_api::TcpListenerResources {
        &mut self.resources.tcp_listeners
    }

    fn tcp_stream_resources(&self) -> &lunatic_networking_api::TcpStreamResources {
        &self.resources.tcp_streams
    }

    fn tcp_stream_resources_mut(&mut self) -> &mut lunatic_networking_api::TcpStreamResources {
        &mut self.resources.tcp_streams
    }

    fn udp_resources(&self) -> &lunatic_networking_api::UdpResources {
        &self.resources.udp_sockets
    }

    fn udp_resources_mut(&mut self) -> &mut lunatic_networking_api::UdpResources {
        &mut self.resources.udp_sockets
    }

    fn dns_resources(&self) -> &lunatic_networking_api::DnsResources {
        &self.resources.dns_iterators
    }

    fn dns_resources_mut(&mut self) -> &mut lunatic_networking_api::DnsResources {
        &mut self.resources.dns_iterators
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
    pub(crate) udp_sockets: HashMapId<Arc<UdpSocket>>,
    pub(crate) errors: HashMapId<anyhow::Error>,
}
