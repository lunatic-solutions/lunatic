pub mod connection;
pub mod control;
pub mod distributed;

use anyhow::Result;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct DistributedProcessState {
    node_id: u64,
    pub control: control::Client,
    pub distributed_client: distributed::Client,
}

impl DistributedProcessState {
    pub async fn new(
        node_id: u64,
        control_client: control::Client,
        distributed_client: distributed::Client,
    ) -> Result<Self> {
        Ok(Self {
            node_id,
            control: control_client,
            distributed_client,
        })
    }

    pub fn node_id(&self) -> u64 {
        self.node_id
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: SocketAddr,
}
