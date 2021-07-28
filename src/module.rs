use anyhow::{anyhow, Result};
use tokio::{
    sync::{broadcast, mpsc::unbounded_channel},
    task::JoinHandle,
};
use uuid::Uuid;
use wasmtime::{Store, Val};

use std::sync::Arc;

use crate::{
    environment::GALLON_IN_INSTRUCTIONS,
    message::Message,
    process::{self, ProcessHandle, Signal},
    state::ProcessState,
    Environment,
};

#[derive(Clone)]
pub struct Module {
    inner: Arc<InnerModule>,
}

struct InnerModule {
    data: Vec<u8>,
    env: Environment,
    wasmtime_module: wasmtime::Module,
}

impl Module {
    pub fn new(data: Vec<u8>, env: Environment, wasmtime_module: wasmtime::Module) -> Self {
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
    pub async fn spawn(
        &self,
        function: &str,
        params: Vec<Val>,
        link: Option<ProcessHandle>,
    ) -> Result<(JoinHandle<()>, ProcessHandle)> {
        // TODO: Switch to new_v1() for distributed Lunatic to assure uniqueness across nodes.
        let id = Uuid::new_v4();
        let (message_sender, message_mailbox) = unbounded_channel::<Message>();
        let (signal_sender, signal_mailbox) = unbounded_channel::<Signal>();
        let (trapped_sender, _) = broadcast::channel(1);
        let mut state = ProcessState::new(
            id,
            trapped_sender.clone(),
            self.clone(),
            message_sender.clone(),
            message_mailbox,
            signal_sender.clone(),
        )?;
        if let Some(link) = link {
            // If processes are linked add the parent as the 0 process of the child.
            state.resources.processes.add(link.clone());
            // Send signal to itself to perform the linking
            signal_sender
                .send(Signal::Link(link))
                .expect("receiver must exist at this point");
        }

        let mut store = Store::new(&self.environment().engine(), state);
        store.limiter(|state| state);

        // Trap if out of fuel
        store.out_of_fuel_trap();
        // Define maximum fuel
        match self.environment().config().max_fuel() {
            Some(max_fuel) => store.out_of_fuel_async_yield(max_fuel, GALLON_IN_INSTRUCTIONS),
            // If no limit is specified use maximum
            None => store.out_of_fuel_async_yield(u64::MAX, GALLON_IN_INSTRUCTIONS),
        };

        let instance = self
            .environment()
            .linker()
            .instantiate_async(&mut store, &self.wasmtime_module())
            .await?;
        let entry = instance
            .get_func(&mut store, &function)
            .map_or(Err(anyhow!("Function '{}' not found", function)), |func| {
                Ok(func)
            })?;

        let fut = async move { entry.call_async(&mut store, &params).await };
        let process = process::new(
            fut,
            message_sender.clone(),
            trapped_sender.clone(),
            signal_mailbox,
        );

        Ok((
            process,
            ProcessHandle::new(id, signal_sender, message_sender, trapped_sender),
        ))
    }

    pub fn environment(&self) -> &Environment {
        &self.inner.env
    }

    pub fn wasmtime_module(&self) -> &wasmtime::Module {
        &self.inner.wasmtime_module
    }

    pub fn data(&self) -> Vec<u8> {
        self.inner.data.clone()
    }
}
