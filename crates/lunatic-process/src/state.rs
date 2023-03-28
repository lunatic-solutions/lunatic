use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use hash_map_id::HashMapId;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    Mutex, RwLock, RwLockWriteGuard,
};
use wasmtime::Linker;

use crate::{
    config::ProcessConfig,
    mailbox::MessageMailbox,
    runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime},
    Signal,
};

pub type ConfigResources<T> = HashMapId<T>;
pub type SignalSender = UnboundedSender<Signal>;
pub type SignalReceiver = Arc<Mutex<UnboundedReceiver<Signal>>>;

/// The internal state of a process.
///
/// The `ProcessState` has two main roles:
/// - It holds onto all vm resources (file descriptors, tcp streams, channels, ...)
/// - Registers all host functions working on those resources to the `Linker`
pub trait ProcessState: Sized {
    type Config: ProcessConfig + Default + Send + Sync;

    // Create a new `ProcessState` using the parent's state (self) to inherit environment and
    // other parts of the state.
    // This is used in the guest function `spawn` which uses this trait and not the concrete state.
    fn new_state(
        &self,
        module: Arc<WasmtimeCompiledModule<Self>>,
        config: Arc<Self::Config>,
    ) -> Result<Self>;

    /// Register all host functions to the linker.
    fn register(linker: &mut Linker<Self>) -> Result<()>;
    /// Marks a wasm instance as initialized
    fn initialize(&mut self);
    /// Returns true if the instance was initialized
    fn is_initialized(&self) -> bool;

    /// Returns the WebAssembly runtime
    fn runtime(&self) -> &WasmtimeRuntime;
    // Returns the WebAssembly module
    fn module(&self) -> &Arc<WasmtimeCompiledModule<Self>>;
    /// Returns the process configuration
    fn config(&self) -> &Arc<Self::Config>;

    // Returns process ID
    fn id(&self) -> u64;
    // Returns signal mailbox
    fn signal_mailbox(&self) -> &(SignalSender, SignalReceiver);
    // Returns message mailbox
    fn message_mailbox(&self) -> &MessageMailbox;

    // Config resources
    fn config_resources(&self) -> &ConfigResources<Self::Config>;
    fn config_resources_mut(&mut self) -> &mut ConfigResources<Self::Config>;

    // Registry
    fn registry(&self) -> &Arc<RwLock<HashMap<String, (u64, u64)>>>;
    fn registry_atomic_put(
        &mut self,
    ) -> &mut Option<RwLockWriteGuard<'static, HashMap<String, (u64, u64)>>>;

    // Spans
    // fn span_resources(&self) -> &SpanResources<>
}
