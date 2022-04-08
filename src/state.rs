use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use async_std::channel::{unbounded, Sender};
use async_std::net::{TcpListener, TcpStream, UdpSocket};
use hash_map_id::HashMapId;
use lunatic_common_api::actor::ActorCtx;
use lunatic_common_api::control::GetNodeIds;
use lunatic_error_api::{ErrorCtx, ErrorResource};
use lunatic_networking_api::dns::DnsIterator;
use lunatic_networking_api::NetworkingCtx;
use lunatic_process::config::ProcessConfig;
use lunatic_process::env::Environment;
use lunatic_process::local_control::local_control;
use lunatic_process::runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime};
use lunatic_process::state::ProcessState;
use lunatic_process::{mailbox::MessageMailbox, message::Message, Signal};
use lunatic_process_api::ProcessCtx;
use lunatic_wasi_api::{build_wasi, LunaticWasiCtx};
use wasmtime::{Linker, ResourceLimiter};
use wasmtime_wasi::WasiCtx;

use crate::DefaultProcessConfig;

pub struct DefaultProcessState {
    // Process id
    pub(crate) id: u64,
    pub(crate) environment: Environment,
    // The WebAssembly runtime
    runtime: Option<WasmtimeRuntime>,
    // The module that this process was spawned from
    pub(crate) module: Option<WasmtimeCompiledModule<Self>>,
    // The process configuration
    pub(crate) config: Arc<DefaultProcessConfig>,
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
    type Config = DefaultProcessConfig;

    fn new(
        environment: Environment,
        runtime: WasmtimeRuntime,
        module: WasmtimeCompiledModule<Self>,
        config: Arc<DefaultProcessConfig>,
        signal_mailbox: Sender<Signal>,
        message_mailbox: MessageMailbox,
    ) -> Result<Self> {
        let state = Self {
            id: environment.get_next_process_id(),
            environment,
            runtime: Some(runtime),
            module: Some(module),
            config: config.clone(),
            message: None,
            signal_mailbox,
            message_mailbox,
            resources: Resources::default(),
            wasi: build_wasi(
                Some(config.command_line_arguments()),
                Some(config.environment_variables()),
                config.preopened_dirs(),
            )?,
            initialized: false,
        };
        Ok(state)
    }

    fn state_for_instantiation() -> Self {
        let config = DefaultProcessConfig::default();
        let (signal_mailbox, _) = unbounded();
        let message_mailbox = MessageMailbox::default();
        Self {
            id: 1,
            environment: Environment::new(0, local_control()),
            runtime: None,
            module: None,
            config: Arc::new(config.clone()),
            message: None,
            signal_mailbox,
            message_mailbox,
            resources: Resources::default(),
            wasi: build_wasi(
                Some(config.command_line_arguments()),
                Some(config.environment_variables()),
                config.preopened_dirs(),
            )
            .unwrap(),
            initialized: false,
        }
    }

    fn register(linker: &mut Linker<Self>) -> Result<()> {
        lunatic_error_api::register(linker)?;
        lunatic_process_api::register(linker)?;
        lunatic_messaging_api::register(linker)?;
        lunatic_networking_api::register(linker)?;
        lunatic_version_api::register(linker)?;
        lunatic_wasi_api::register(linker)?;
        Ok(())
    }

    fn initialize(&mut self) {
        self.initialized = true;
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn runtime(&self) -> &WasmtimeRuntime {
        self.runtime.as_ref().unwrap()
    }

    fn config(&self) -> &Arc<DefaultProcessConfig> {
        &self.config
    }

    fn module(&self) -> &WasmtimeCompiledModule<Self> {
        self.module.as_ref().unwrap()
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn node_id(&self) -> u64 {
        0 // TODO
    }

    fn signal_mailbox(&self) -> &Sender<Signal> {
        &self.signal_mailbox
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
        desired <= self.config().get_max_memory()
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

impl ProcessCtx<DefaultProcessState> for DefaultProcessState {
    fn mailbox(&mut self) -> &mut MessageMailbox {
        &mut self.message_mailbox
    }

    fn message_scratch_area(&mut self) -> &mut Option<Message> {
        &mut self.message
    }

    fn module_resources(&self) -> &lunatic_process_api::ModuleResources<DefaultProcessState> {
        &self.resources.modules
    }

    fn module_resources_mut(
        &mut self,
    ) -> &mut lunatic_process_api::ModuleResources<DefaultProcessState> {
        &mut self.resources.modules
    }

    fn config_resources(
        &self,
    ) -> &lunatic_process_api::ConfigResources<<DefaultProcessState as ProcessState>::Config> {
        &self.resources.configs
    }

    fn config_resources_mut(
        &mut self,
    ) -> &mut lunatic_process_api::ConfigResources<<DefaultProcessState as ProcessState>::Config>
    {
        &mut self.resources.configs
    }

    fn environment(&self) -> &lunatic_process::env::Environment {
        &self.environment
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

impl ActorCtx<GetNodeIds> for DefaultProcessState {
    fn actor(&self) -> lunatic_common_api::actor::ActorHandle<GetNodeIds> {
        todo!()
    }
}

#[derive(Default, Debug)]
pub(crate) struct Resources {
    pub(crate) configs: HashMapId<DefaultProcessConfig>,
    pub(crate) modules: HashMapId<WasmtimeCompiledModule<DefaultProcessState>>,
    pub(crate) dns_iterators: HashMapId<DnsIterator>,
    pub(crate) tcp_listeners: HashMapId<TcpListener>,
    pub(crate) tcp_streams: HashMapId<TcpStream>,
    pub(crate) udp_sockets: HashMapId<Arc<UdpSocket>>,
    pub(crate) errors: HashMapId<anyhow::Error>,
}

mod tests {
    #[async_std::test]
    async fn import_filter_signature_matches() {
        use crate::state::DefaultProcessState;
        use crate::DefaultProcessConfig;
        use lunatic_process::runtimes::wasmtime::WasmtimeRuntime;
        use std::sync::Arc;

        // The default configuration includes both, the "lunatic::*" and "wasi_*" namespaces.
        let config = DefaultProcessConfig::default();

        // Create wasmtime runtime
        let mut wasmtime_config = wasmtime::Config::new();
        wasmtime_config.async_support(true).consume_fuel(true);
        let runtime = WasmtimeRuntime::new(&wasmtime_config).unwrap();

        let raw_module = wat::parse_file("./wat/all_imports.wat").unwrap();
        let module = runtime
            .compile_module::<DefaultProcessState>(raw_module)
            .unwrap();

        let env = lunatic_process::env::Environment::local();
        env.spawn_wasm(runtime, module, Arc::new(config), "hello", Vec::new(), None)
            .await
            .unwrap();
    }
}
