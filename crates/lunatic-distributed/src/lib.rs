pub mod control;
pub mod distributed;
pub mod quic;

use anyhow::Result;
use lunatic_process::{
    env::Environment,
    runtimes::wasmtime::{WasmtimeCompiledModule, WasmtimeRuntime},
    state::ProcessState,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};

pub trait DistributedCtx<E: Environment>: ProcessState + Sized {
    fn new_dist_state(
        environment: Arc<E>,
        distributed: DistributedProcessState,
        runtime: WasmtimeRuntime,
        module: Arc<WasmtimeCompiledModule<Self>>,
        config: Arc<Self::Config>,
    ) -> Result<Self>;
    fn distributed(&self) -> Result<&DistributedProcessState>;
    fn distributed_mut(&mut self) -> Result<&mut DistributedProcessState>;
    fn module_id(&self) -> u64;
    fn environment_id(&self) -> u64;
    fn can_spawn(&self) -> bool;
}

#[derive(Clone)]
pub struct DistributedProcessState {
    node_id: u64,
    pub control: control::Client,
    pub node_client: distributed::Client,
}

impl DistributedProcessState {
    pub async fn new(
        node_id: u64,
        control_client: control::Client,
        node_client: distributed::Client,
    ) -> Result<Self> {
        Ok(Self {
            node_id,
            control: control_client,
            node_client,
        })
    }

    pub fn node_id(&self) -> u64 {
        self.node_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: SocketAddr,
    pub name: String,
}
