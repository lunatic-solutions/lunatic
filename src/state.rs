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
use lunatic_wasi_api::{build_wasi, LunaticWasiCtx};
use uuid::Uuid;
use wasmtime::{Linker, ResourceLimiter};
use wasmtime_wasi::WasiCtx;

use crate::api::process;
use crate::module::Module;
use crate::{EnvConfig, Environment};

/// The internal state of a process.
///
/// The `ProcessState` has two main roles:
/// - It holds onto all vm resources (file descriptors, tcp streams, channels, ...)
/// - Registers all host functions working on those resources to the `Linker`
pub trait ProcessState: Sized {
    /// Register all host functions to the linker.
    fn register(linker: &mut Linker<Self>, namespace_filter: &[String]) -> Result<()>;
}

// The default process state
pub(crate) struct DefaultProcessState {
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

impl ProcessState for DefaultProcessState {
    fn register(linker: &mut Linker<Self>, namespace_filter: &[String]) -> Result<()> {
        lunatic_error_api::register(linker, namespace_filter)?;
        process::register(linker, namespace_filter)?;
        lunatic_messaging_api::register(linker, namespace_filter)?;
        lunatic_networking_api::register(linker, namespace_filter)?;
        lunatic_version_api::register(linker, namespace_filter)?;
        lunatic_wasi_api::register(linker, namespace_filter)?;
        Ok(())
    }
}

impl DefaultProcessState {
    pub fn new(
        id: Uuid,
        module: Module,
        signal_mailbox: Sender<Signal>,
        message_mailbox: MessageMailbox,
        config: &EnvConfig,
    ) -> Result<Self> {
        let state = Self {
            id,
            module,
            message: None,
            signal_mailbox,
            message_mailbox,
            resources: Resources::default(),
            wasi: build_wasi(
                config.wasi_args().as_ref(),
                config.wasi_envs().as_ref(),
                config.preopened_dirs(),
            )?,
            initialized: false,
        };
        Ok(state)
    }
}

impl Debug for DefaultProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("process", &self.resources)
            .finish()
    }
}

// Limit the maximum memory of the process depending on the environment it was spawned in.
impl ResourceLimiter for DefaultProcessState {
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

impl ErrorCtx for DefaultProcessState {
    fn error_resources(&self) -> &ErrorResource {
        &self.resources.errors
    }

    fn error_resources_mut(&mut self) -> &mut ErrorResource {
        &mut self.resources.errors
    }
}

impl ProcessCtx for DefaultProcessState {
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

impl NetworkingCtx for DefaultProcessState {
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

impl LunaticWasiCtx for DefaultProcessState {
    fn wasi(&mut self) -> &mut WasiCtx {
        &mut self.wasi
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

mod tests {
    #[async_std::test]
    async fn import_filter_signature_matches() {
        use crate::{EnvConfig, Environment};

        // The default configuration includes both, the "lunatic::*" and "wasi_*" namespaces.
        let config = EnvConfig::default();
        let environment = Environment::local(config).unwrap();
        let raw_module = wat::parse_file("./wat/all_imports.wat").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();

        // This configuration should still compile, even all host calls will trap.
        let config = EnvConfig::new(0, None);
        let environment = Environment::local(config).unwrap();
        let raw_module = wat::parse_file("./wat/all_imports.wat").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();
    }
}
