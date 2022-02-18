/*! Wasm modules */

use anyhow::{anyhow, Result};
use async_std::channel::unbounded;
use async_std::task::JoinHandle;
use log::trace;
use lunatic_process::mailbox::MessageMailbox;
use lunatic_process::{self, Process, Signal, WasmProcess};
use uuid::Uuid;
use wasmtime::{Store, Val};

use std::sync::Arc;

use crate::node::{Link, ProxyProcess};
use crate::{
    environment::{EnvironmentLocal, UNIT_OF_COMPUTE_IN_INSTRUCTIONS},
    node::Peer,
    state::DefaultProcessState,
};

#[derive(Clone)]
pub enum Module {
    Local(ModuleLocal),
    Remote(ModuleRemote),
}

impl Module {
    pub fn local(data: Vec<u8>, env: EnvironmentLocal, wasmtime_module: wasmtime::Module) -> Self {
        Self::Local(ModuleLocal::new(data, env, wasmtime_module))
    }
    pub async fn remote(env_id: u64, peer: Peer, data: Vec<u8>) -> Result<Self> {
        Ok(Self::Remote(ModuleRemote::new(env_id, peer, data).await?))
    }
    pub fn environment(&self) -> &EnvironmentLocal {
        match self {
            Module::Local(local) => local.environment(),
            Module::Remote(_) => unreachable!("Can't grab local environment on remote node"),
        }
    }
    pub fn data(&self) -> Vec<u8> {
        match self {
            Module::Local(local) => local.data(),
            Module::Remote(_) => unreachable!("Can't grab module data on remote node"),
        }
    }
    pub async fn spawn<'a>(
        &'a self,
        function: &'a str,
        params: Vec<Val>,
        link: Option<(Option<i64>, Arc<dyn Process>)>,
    ) -> Result<(JoinHandle<()>, Arc<dyn Process>)> {
        match self {
            Module::Local(local) => local.spawn(function, params, link).await,
            Module::Remote(remote) => remote.spawn(function, params, link).await,
        }
    }
}

#[derive(Clone)]
pub struct ModuleRemote {
    id: u64,
    peer: Peer,
}

impl ModuleRemote {
    pub async fn new(env_id: u64, peer: Peer, data: Vec<u8>) -> Result<Self> {
        let id = peer.create_module(env_id, data).await?;
        Ok(Self { id, peer })
    }
    pub async fn spawn<'a>(
        &'a self,
        function: &'a str,
        params: Vec<Val>,
        link: Option<(Option<i64>, Arc<dyn Process>)>,
    ) -> Result<(JoinHandle<()>, Arc<dyn Process>)> {
        let params: Option<Vec<i32>> = params.into_iter().map(|param| param.i32()).collect();
        if params.is_none() {
            return Err(anyhow!(
                "Only i32 arguments can be sent over the network the the spawn function"
            ));
        }
        let node = {
            crate::NODE
                .write()
                .await
                .as_ref()
                .expect("Must exist if remote environment exists")
                .clone()
        };
        let link = if let Some((tag, process)) = link {
            let link_id = node
                .inner
                .write()
                .await
                .resources
                .add(crate::node::Resource::Process(process));
            let link = Link::new(tag, link_id);
            Some(link)
        } else {
            None
        };
        let remote_id = self
            .peer
            .spawn(self.id, function.to_string(), params.unwrap(), link)
            .await?;
        let (handle, proxy_process) = ProxyProcess::new(remote_id, self.peer.clone(), node);
        Ok((handle, Arc::new(proxy_process)))
    }
}

/// A compiled WebAssembly module that can be used to spawn [`WasmProcesses`][0].
///
/// Modules are created from [`Environments`](crate::environment::Environment).
///
/// [0]: crate::WasmProcess
#[derive(Clone)]
pub struct ModuleLocal {
    inner: Arc<InnerModule>,
}

struct InnerModule {
    data: Vec<u8>,
    env: EnvironmentLocal,
    wasmtime_module: wasmtime::Module,
}

impl ModuleLocal {
    pub(crate) fn new(
        data: Vec<u8>,
        env: EnvironmentLocal,
        wasmtime_module: wasmtime::Module,
    ) -> Self {
        Self {
            inner: Arc::new(InnerModule {
                data,
                env,
                wasmtime_module,
            }),
        }
    }

    /// Spawns a new process from the module.
    ///
    /// A `Process` is created from a `Module`, an entry `function` and an array of arguments. The
    /// configuration of the environment will define some characteristics of the process, such as
    /// maximum memory, fuel and available host functions.
    ///
    /// After it's spawned the process will keep running in the background. A process can be killed
    /// by sending a `Signal::Kill` to it. If you would like to block until the process is finished
    /// you can `.await` on the returned `JoinHandle<()>`.
    ///
    /// Note: The 'a lifetime is here just because Rust has a bug in handling `dyn Trait` in async:
    /// https://github.com/rust-lang/rust/issues/63033
    /// If it ever becomes an issue there are other workarounds that could be used instead.
    pub async fn spawn<'a>(
        &'a self,
        function: &'a str,
        params: Vec<Val>,
        link: Option<(Option<i64>, Arc<dyn Process>)>,
    ) -> Result<(JoinHandle<()>, Arc<dyn Process>)> {
        // TODO: Switch to new_v1() for distributed Lunatic to assure uniqueness across nodes.
        let id = Uuid::new_v4();
        trace!("Spawning process: {}", id);
        let signal_mailbox = unbounded::<Signal>();
        let message_mailbox = MessageMailbox::default();
        let state = DefaultProcessState::new(
            id,
            Module::Local(self.clone()),
            signal_mailbox.0.clone(),
            message_mailbox.clone(),
            self.environment().config(),
        )?;

        let mut store = Store::new(self.environment().engine(), state);
        store.limiter(|state| state);

        // Trap if out of fuel
        store.out_of_fuel_trap();
        // Define maximum fuel
        match self.environment().config().max_fuel() {
            Some(max_fuel) => {
                store.out_of_fuel_async_yield(max_fuel, UNIT_OF_COMPUTE_IN_INSTRUCTIONS)
            }
            // If no limit is specified use maximum
            None => store.out_of_fuel_async_yield(u64::MAX, UNIT_OF_COMPUTE_IN_INSTRUCTIONS),
        };

        let instance = self
            .environment()
            .linker()
            .instantiate_async(&mut store, self.wasmtime_module())
            .await?;
        // Once the module is initialized, set state.initialized to true
        let state = store.data_mut();
        state.initialized = true;

        let entry = instance
            .get_func(&mut store, function)
            .map_or(Err(anyhow!("Function '{}' not found", function)), |func| {
                Ok(func)
            })?;

        let fut = async move { entry.call_async(&mut store, &params, &mut []).await };
        let child_process = lunatic_process::new(fut, id, signal_mailbox.1, message_mailbox);
        let child_process_handle = WasmProcess::new(id, signal_mailbox.0.clone());

        // **Child link guarantees**:
        // The link signal is going to be put inside of the child's mailbox and is going to be
        // processed before any child code can run. This means that any failure inside the child
        // Wasm code will be correctly reported to the parent.
        //
        // We assume here that the code inside of `process::new()` will not fail during signal
        // handling.
        //
        // **Parent link guarantees**:
        // A `tokio::task::yield_now()` call is executed to allow the parent to link the child
        // before continuing any further execution. This should force the parent to process all
        // signals right away.
        //
        // The parent could have received a `kill` signal in its mailbox before this function was
        // called and this signal is going to be processed before the link is established (FIFO).
        // Only after the yield function we can guarantee that the child is going to be notified
        // if the parent fails. This is ok, as the actual spawning of the child happens after the
        // call, so the child wouldn't even exist if the parent failed before.
        //
        // TODO: The guarantees provided here don't hold anymore in a distributed environment and
        //       will require some rethinking. This function will be executed on a completly
        //       different computer and needs to be synced in a more robust way with the parent
        //       running somewhere else.
        if let Some((tag, process)) = link {
            // Send signal to itself to perform the linking
            process.send(Signal::Link(None, Arc::new(child_process_handle.clone())));
            // Suspend itself to process all new signals
            async_std::task::yield_now().await;
            // Send signal to child to link it
            signal_mailbox
                .0
                .try_send(Signal::Link(tag, process))
                .expect("receiver must exist at this point");
        }

        // Spawn a background process
        trace!("Process size: {}", std::mem::size_of_val(&child_process));
        let join = async_std::task::spawn(child_process);
        Ok((join, Arc::new(child_process_handle)))
    }

    pub fn environment(&self) -> &EnvironmentLocal {
        &self.inner.env
    }

    pub fn wasmtime_module(&self) -> &wasmtime::Module {
        &self.inner.wasmtime_module
    }

    /// The raw WebAssembly data that the Module was created from.
    pub fn data(&self) -> Vec<u8> {
        self.inner.data.clone()
    }
}
