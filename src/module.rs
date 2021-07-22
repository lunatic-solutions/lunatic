use anyhow::{anyhow, Result};
use tokio::sync::mpsc::unbounded_channel;
use wasmtime::{Store, Val};

use std::sync::Arc;

use crate::{
    environment::GALLON_IN_INSTRUCTIONS, message::Message, process::ProcessHandle,
    state::ProcessState, Environment,
};

#[derive(Clone)]
pub struct Module {
    inner: Arc<InnerModule>,
}

struct InnerModule {
    env: Environment,
    wasmtime_module: wasmtime::Module,
}

impl Module {
    pub fn new(env: Environment, wasmtime_module: wasmtime::Module) -> Self {
        Self {
            inner: Arc::new(InnerModule {
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
    /// by sending a `Signal::Kill` to it.
    pub async fn spawn(&self, function: &str, params: Vec<Val>) -> Result<ProcessHandle> {
        let (mailbox_sender, mailbox) = unbounded_channel::<Message>();
        let state = ProcessState::new(self.clone(), mailbox);
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
        Ok(ProcessHandle::new(fut, mailbox_sender))
    }

    pub fn environment(&self) -> &Environment {
        &self.inner.env
    }

    pub fn wasmtime_module(&self) -> &wasmtime::Module {
        &self.inner.wasmtime_module
    }
}
