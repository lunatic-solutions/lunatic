use anyhow::{anyhow, Result};
use wasmtime::{Config, Engine, InstanceAllocationStrategy, Linker, OptLevel, ProfilingStrategy};

use super::config::EnvConfig;
use crate::{api, module::Module, node::Peer, registry::EnvRegistry, state::ProcessState};

// One unit of fuel represents around 100k instructions.
pub const UNIT_OF_COMPUTE_IN_INSTRUCTIONS: u64 = 100_000;

/// The environment represents a set of characteristics that processes spawned from it will have.
///
/// Environments let us set limits on processes:
/// * Memory limits
/// * Compute limits
/// * Access to host functions
#[derive(Clone)]
pub enum Environment {
    Local(Box<EnvironmentLocal>),
    Remote(EnvironmentRemote),
}

impl Environment {
    pub fn local(config: EnvConfig) -> Result<Self> {
        Ok(Self::Local(EnvironmentLocal::new(config)?))
    }
    pub async fn remote(node_name: &str, config: EnvConfig) -> Result<Self> {
        Ok(Self::Remote(
            EnvironmentRemote::new(node_name, config).await?,
        ))
    }
    pub async fn create_module(&self, data: Vec<u8>) -> Result<Module> {
        match self {
            Environment::Local(local) => local.create_module(data).await,
            Environment::Remote(remote) => remote.create_module(remote.id, data).await,
        }
    }
    pub fn registry(&self) -> &EnvRegistry {
        match self {
            Environment::Local(local) => local.registry(),
            Environment::Remote(remote) => remote.registry(),
        }
    }
}

#[derive(Clone)]
pub struct EnvironmentRemote {
    id: u64,
    peer: Peer,
    registry: EnvRegistry,
}

impl EnvironmentRemote {
    pub async fn new(node_name: &str, config: EnvConfig) -> Result<Self> {
        let node = crate::NODE.read().await;
        if node.is_none() {
            return Err(anyhow!(
                "Can't create remote environment on a node not connected to others"
            ));
        }
        let node = node.as_ref().unwrap();
        let node = node.inner.read().await;
        let peer = node.peers.get(node_name);
        if peer.is_none() {
            return Err(anyhow!(
                "Can't create remote environment, node doesn't exist"
            ));
        }
        let peer = peer.unwrap().clone();
        let id = peer.create_environment(config).await?;
        Ok(Self {
            id,
            peer: peer.clone(),
            registry: EnvRegistry::remote(id, peer),
        })
    }

    pub async fn create_module(&self, env_id: u64, data: Vec<u8>) -> Result<Module> {
        Ok(Module::remote(env_id, self.peer.clone(), data).await?)
    }

    pub fn registry(&self) -> &EnvRegistry {
        &self.registry
    }
}

#[derive(Clone)]
pub struct EnvironmentLocal {
    engine: Engine,
    linker: Linker<ProcessState>,
    config: EnvConfig,
    registry: EnvRegistry,
}

impl EnvironmentLocal {
    /// Create a new environment from a configuration.
    pub fn new(config: EnvConfig) -> Result<Box<Self>> {
        let mut wasmtime_config = Config::new();
        wasmtime_config
            .async_support(true)
            .debug_info(false)
            // The behaviour of fuel running out is defined on the Store
            .consume_fuel(true)
            .wasm_reference_types(true)
            .wasm_bulk_memory(true)
            .wasm_multi_value(true)
            .wasm_multi_memory(true)
            .wasm_module_linking(false)
            // Disable profiler
            .profiler(ProfilingStrategy::None)?
            .cranelift_opt_level(OptLevel::SpeedAndSize)
            // Allocate resources on demand because we can't predict how many process will exist
            .allocation_strategy(InstanceAllocationStrategy::OnDemand)
            // Always use static memories
            .static_memory_forced(true)
            // Limit static memories to the maximum possible size in the environment
            .static_memory_maximum_size(config.max_memory() as u64)
            // Set memory guards to 4 Mb
            .static_memory_guard_size(0x400000);
        let engine = Engine::new(&wasmtime_config)?;
        let mut linker = Linker::new(&engine);
        // Allow plugins to shadow host functions
        linker.allow_shadowing(true);

        // Register host functions for linker
        api::register(&mut linker, config.allowed_namespace())?;

        Ok(Box::new(Self {
            engine,
            linker,
            config,
            registry: EnvRegistry::local(),
        }))
    }

    /// Create a module from the environment.
    ///
    /// All plugins in this environment will get instantiated and their `lunatic_create_module_hook`
    /// function will be called. Plugins can use host functions to modify the module before it's JIT
    /// compiled by `Wasmtime`.
    pub async fn create_module(&self, data: Vec<u8>) -> Result<Module> {
        let env = self.clone();
        // The compilation of a module is a CPU intensive tasks and can take some time.
        let module = async_std::task::spawn_blocking(move || {
            match wasmtime::Module::new(env.engine(), data.as_slice()) {
                Ok(wasmtime_module) => Ok(Module::local(data, env, wasmtime_module)),
                Err(err) => Err(err),
            }
        })
        .await?;
        Ok(module)
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn config(&self) -> &EnvConfig {
        &self.config
    }

    pub(crate) fn linker(&self) -> &Linker<ProcessState> {
        &self.linker
    }

    pub fn registry(&self) -> &EnvRegistry {
        &self.registry
    }
}
