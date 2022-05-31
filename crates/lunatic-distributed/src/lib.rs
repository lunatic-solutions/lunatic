pub mod connection;
pub mod control;
pub mod distributed;

use anyhow::Result;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct DistributedProcessState {
    pub node_id: u64,
    pub control_client: control::Client,
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
            control_client,
            distributed_client,
        })
    }

    pub async fn get_nodes(&self) -> Vec<NodeInfo> {
        // TODO cache the list?
        self.control_client.get_nodes().await
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: SocketAddr,
}
