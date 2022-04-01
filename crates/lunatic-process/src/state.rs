use std::sync::Arc;

use anyhow::Result;
use async_std::channel::{Receiver, Sender};
use hash_map_id::HashMapId;
use uuid::Uuid;
use wasmtime::Linker;

use crate::{
    config::ProcessConfig,
    mailbox::MessageMailbox,
    runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime},
    Signal,
};

pub type ConfigResources<T> = HashMapId<T>;

/// The internal state of a process.
///
/// The `ProcessState` has two main roles:
/// - It holds onto all vm resources (file descriptors, tcp streams, channels, ...)
/// - Registers all host functions working on those resources to the `Linker`
pub trait ProcessState: Sized + Default {
    type Config: ProcessConfig + Default + Send + Sync;

    // Create a new `ProcessState`
    fn new(
        runtime: WasmtimeRuntime,
        module: WasmtimeCompiledModule<Self>,
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
    fn module(&self) -> &WasmtimeCompiledModule<Self>;
    /// Returns the process configuration
    fn config(&self) -> &Arc<Self::Config>;

    // Returns ID
    fn id(&self) -> Uuid;
    // Returns signal mailbox
    fn signal_mailbox(&self) -> &(Sender<Signal>, Receiver<Signal>);
    // Returns message mailbox
    fn message_mailbox(&self) -> &MessageMailbox;

    // Config resources
    fn config_resources(&self) -> &ConfigResources<Self::Config>;
    fn config_resources_mut(&mut self) -> &mut ConfigResources<Self::Config>;
}
