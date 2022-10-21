use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use hash_map_id::HashMapId;
use lunatic_distributed::{DistributedCtx, DistributedProcessState};
use lunatic_error_api::{ErrorCtx, ErrorResource};
use lunatic_networking_api::{DnsIterator, TlsConnection, TlsListener};
use lunatic_networking_api::{NetworkingCtx, TcpConnection};
use lunatic_process::config::ProcessConfig;
use lunatic_process::env::Environment;
use lunatic_process::runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime};
use lunatic_process::state::{ConfigResources, ProcessState};
use lunatic_process::{mailbox::MessageMailbox, message::Message, Signal};
use lunatic_process_api::{ProcessConfigCtx, ProcessCtx};
use lunatic_stdout_capture::StdoutCapture;
use lunatic_timer_api::{TimerCtx, TimerResources};
use lunatic_wasi_api::{build_wasi, LunaticWasiCtx};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use wasmtime::{Linker, ResourceLimiter};
use wasmtime_wasi::WasiCtx;

use crate::DefaultProcessConfig;

pub struct DefaultProcessState {
    // Process id
    pub(crate) id: u64,
    pub(crate) environment: Environment<Self>,
    pub(crate) distributed: Option<DistributedProcessState>,
    // The WebAssembly runtime
    runtime: Option<WasmtimeRuntime>,
    // The module that this process was spawned from
    module: Option<Arc<WasmtimeCompiledModule<Self>>>,
    // The process configuration
    config: Arc<DefaultProcessConfig>,
    // A space that can be used to temporarily store messages when sending or receiving them.
    // Messages can contain resources that need to be added across multiple host. Likewise,
    // receiving messages is done in two steps, first the message size is returned to allow the
    // guest to reserve enough space and then the it's received. Both of those actions use
    // `message` as a temp space to store messages across host calls.
    message: Option<Message<Self>>,
    // Signals sent to the mailbox
    signal_mailbox: (
        UnboundedSender<Signal<Self>>,
        Arc<Mutex<UnboundedReceiver<Signal<Self>>>>,
    ),
    // Messages sent to the process
    message_mailbox: MessageMailbox<Self>,
    // Resources
    resources: Resources,
    // WASI
    wasi: WasiCtx,
    // WASI stdout stream
    wasi_stdout: Option<StdoutCapture>,
    // WASI stderr stream
    wasi_stderr: Option<StdoutCapture>,
    // Set to true if the WASM module has been instantiated
    initialized: bool,
    // Shared process registry
    registry: Arc<DashMap<String, (u64, u64)>>,
}

impl DefaultProcessState {
    pub fn new(
        environment: Environment<Self>,
        distributed: Option<DistributedProcessState>,
        runtime: WasmtimeRuntime,
        module: Arc<WasmtimeCompiledModule<Self>>,
        config: Arc<DefaultProcessConfig>,
        registry: Arc<DashMap<String, (u64, u64)>>,
    ) -> Result<Self> {
        let signal_mailbox = unbounded_channel();
        let signal_mailbox = (signal_mailbox.0, Arc::new(Mutex::new(signal_mailbox.1)));
        let message_mailbox = MessageMailbox::default();
        let state = Self {
            id: environment.get_next_process_id(),
            environment,
            distributed,
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
            wasi_stdout: None,
            wasi_stderr: None,
            initialized: false,
            registry,
        };
        Ok(state)
    }
}

impl ProcessState for DefaultProcessState {
    type Config = DefaultProcessConfig;

    fn new_state(
        &self,
        module: Arc<WasmtimeCompiledModule<Self>>,
        config: Arc<DefaultProcessConfig>,
    ) -> Result<Self> {
        let signal_mailbox = unbounded_channel();
        let signal_mailbox = (signal_mailbox.0, Arc::new(Mutex::new(signal_mailbox.1)));
        let message_mailbox = MessageMailbox::default();
        let state = Self {
            id: self.environment.get_next_process_id(),
            environment: self.environment.clone(),
            distributed: self.distributed.clone(),
            runtime: self.runtime.clone(),
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
            wasi_stdout: None,
            wasi_stderr: None,
            initialized: false,
            registry: self.registry.clone(),
        };
        Ok(state)
    }

    fn state_for_instantiation() -> Self {
        let config = DefaultProcessConfig::default();
        let signal_mailbox = unbounded_channel();
        let signal_mailbox = (signal_mailbox.0, Arc::new(Mutex::new(signal_mailbox.1)));
        let message_mailbox = MessageMailbox::default();
        Self {
            id: 1,
            environment: Environment::new(0),
            distributed: None,
            runtime: None,
            module: None,
            registry: Default::default(),
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
            wasi_stdout: None,
            wasi_stderr: None,
            initialized: false,
        }
    }

    fn register(linker: &mut Linker<Self>) -> Result<()> {
        lunatic_error_api::register(linker)?;
        lunatic_process_api::register(linker)?;
        lunatic_messaging_api::register(linker)?;
        lunatic_timer_api::register(linker)?;
        lunatic_networking_api::register(linker)?;
        lunatic_version_api::register(linker)?;
        lunatic_wasi_api::register(linker)?;
        lunatic_registry_api::register(linker)?;
        lunatic_distributed_api::register(linker)?;
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

    fn module(&self) -> &Arc<WasmtimeCompiledModule<Self>> {
        self.module.as_ref().unwrap()
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn signal_mailbox(
        &self,
    ) -> &(
        UnboundedSender<Signal<Self>>,
        Arc<Mutex<UnboundedReceiver<Signal<Self>>>>,
    ) {
        &self.signal_mailbox
    }

    fn message_mailbox(&self) -> &MessageMailbox<Self> {
        &self.message_mailbox
    }

    fn config_resources(&self) -> &ConfigResources<<DefaultProcessState as ProcessState>::Config> {
        &self.resources.configs
    }

    fn config_resources_mut(
        &mut self,
    ) -> &mut ConfigResources<<DefaultProcessState as ProcessState>::Config> {
        &mut self.resources.configs
    }

    fn registry(&self) -> &Arc<DashMap<String, (u64, u64)>> {
        &self.registry
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

    fn table_growing(&mut self, _current: u32, desired: u32, _maximum: Option<u32>) -> bool {
        desired < 100_000
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
    fn mailbox(&mut self) -> &mut MessageMailbox<Self> {
        &mut self.message_mailbox
    }

    fn message_scratch_area(&mut self) -> &mut Option<Message<Self>> {
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

    fn environment(&self) -> &lunatic_process::env::Environment<Self> {
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

    fn tls_listener_resources(&self) -> &lunatic_networking_api::TlsListenerResources {
        &self.resources.tls_listeners
    }

    fn tls_listener_resources_mut(&mut self) -> &mut lunatic_networking_api::TlsListenerResources {
        &mut self.resources.tls_listeners
    }

    fn tls_stream_resources(&self) -> &lunatic_networking_api::TlsStreamResources {
        &self.resources.tls_streams
    }

    fn tls_stream_resources_mut(&mut self) -> &mut lunatic_networking_api::TlsStreamResources {
        &mut self.resources.tls_streams
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

impl TimerCtx for DefaultProcessState {
    fn timer_resources(&self) -> &TimerResources {
        &self.resources.timers
    }

    fn timer_resources_mut(&mut self) -> &mut TimerResources {
        &mut self.resources.timers
    }
}

impl LunaticWasiCtx for DefaultProcessState {
    fn wasi(&self) -> &WasiCtx {
        &self.wasi
    }

    fn wasi_mut(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    // Redirect the stdout stream
    fn set_stdout(&mut self, stdout: StdoutCapture) {
        self.wasi_stdout = Some(stdout.clone());
        self.wasi.set_stdout(Box::new(stdout));
    }

    // Redirect the stderr stream
    fn set_stderr(&mut self, stderr: StdoutCapture) {
        self.wasi_stderr = Some(stderr.clone());
        self.wasi.set_stderr(Box::new(stderr));
    }

    fn get_stdout(&self) -> Option<&StdoutCapture> {
        self.wasi_stdout.as_ref()
    }

    fn get_stderr(&self) -> Option<&StdoutCapture> {
        self.wasi_stderr.as_ref()
    }
}

#[derive(Default, Debug)]
pub(crate) struct Resources {
    pub(crate) configs: HashMapId<DefaultProcessConfig>,
    pub(crate) modules: HashMapId<Arc<WasmtimeCompiledModule<DefaultProcessState>>>,
    pub(crate) timers: TimerResources,
    pub(crate) dns_iterators: HashMapId<DnsIterator>,
    pub(crate) tcp_listeners: HashMapId<TcpListener>,
    pub(crate) tcp_streams: HashMapId<Arc<TcpConnection>>,
    pub(crate) tls_listeners: HashMapId<TlsListener>,
    pub(crate) tls_streams: HashMapId<Arc<TlsConnection>>,
    pub(crate) udp_sockets: HashMapId<Arc<UdpSocket>>,
    pub(crate) errors: HashMapId<anyhow::Error>,
}

impl DistributedCtx for DefaultProcessState {
    fn distributed_mut(&mut self) -> Result<&mut DistributedProcessState> {
        match self.distributed.as_mut() {
            Some(d) => Ok(d),
            None => Err(anyhow::anyhow!("Distributed is not initialized")),
        }
    }

    fn distributed(&self) -> Result<&DistributedProcessState> {
        match self.distributed.as_ref() {
            Some(d) => Ok(d),
            None => Err(anyhow::anyhow!("Distributed is not initialized")),
        }
    }

    fn module_id(&self) -> u64 {
        self.module
            .as_ref()
            .and_then(|m| m.source().id)
            .unwrap_or(0)
    }

    fn environment_id(&self) -> u64 {
        self.environment.id()
    }

    fn can_spawn(&self) -> bool {
        self.config().can_spawn_processes()
    }

    fn new_dist_state(
        environment: Environment<Self>,
        distributed: DistributedProcessState,
        runtime: WasmtimeRuntime,
        module: Arc<WasmtimeCompiledModule<Self>>,
        config: Arc<Self::Config>,
    ) -> Result<Self> {
        let signal_mailbox = unbounded_channel();
        let signal_mailbox = (signal_mailbox.0, Arc::new(Mutex::new(signal_mailbox.1)));
        let message_mailbox = MessageMailbox::default();
        let state = Self {
            id: environment.get_next_process_id(),
            environment,
            distributed: Some(distributed),
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
            wasi_stdout: None,
            wasi_stderr: None,
            initialized: false,
            registry: Default::default(), // TODO move registry into env?
        };
        Ok(state)
    }
}

mod tests {

    #[tokio::test]
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
        let module = Arc::new(runtime.compile_module(raw_module.into()).unwrap());
        let env = lunatic_process::env::Environment::new(0);
        let registry = Arc::new(dashmap::DashMap::new());
        let state = DefaultProcessState::new(
            env.clone(),
            None,
            runtime.clone(),
            module.clone(),
            Arc::new(config),
            registry,
        )
        .unwrap();

        env.spawn_wasm(runtime, &module, state, "hello", Vec::new(), None)
            .await
            .unwrap();
    }
}
