use anyhow::{anyhow, Result};
use tokio::{sync::mpsc::unbounded_channel, task::JoinHandle};
use uuid::Uuid;
use wasmtime::{Store, Val};

use std::sync::Arc;

use crate::{
    environment::UNIT_OF_COMPUTE_IN_INSTRUCTIONS,
    mailbox::MessageMailbox,
    process::{self, Signal, WasmProcess},
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
        link: Option<(Option<i64>, WasmProcess)>,
    ) -> Result<(JoinHandle<()>, WasmProcess)> {
        // TODO: Switch to new_v1() for distributed Lunatic to assure uniqueness across nodes.
        let id = Uuid::new_v4();
        let signal_mailbox = unbounded_channel::<Signal>();
        let message_mailbox = MessageMailbox::default();
        let state = ProcessState::new(
            id,
            self.clone(),
            signal_mailbox.0.clone(),
            message_mailbox.clone(),
        )?;
        if let Some((tag, process)) = link {
            // Send signal to itself to perform the linking
            signal_mailbox
                .0
                .send(Signal::Link(tag, process))
                .expect("receiver must exist at this point");
        }

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
        let entry = instance
            .get_func(&mut store, function)
            .map_or(Err(anyhow!("Function '{}' not found", function)), |func| {
                Ok(func)
            })?;

        let fut = async move { entry.call_async(&mut store, &params).await };
        let process = process::new(fut, signal_mailbox.1, message_mailbox);

        Ok((process, WasmProcess::new(id, signal_mailbox.0)))
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
