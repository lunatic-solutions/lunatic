pub mod client;
pub mod connection;
pub mod message;
pub mod server;

use std::net::SocketAddr;

use self::{
    client::Client,
    message::{Request, Response},
};

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: SocketAddr,
}

#[derive(Clone)]
pub struct ControlInterface {
    pub node_id: u64,
    client: Client,
}

impl ControlInterface {
    pub async fn get_nodes(&self) -> Vec<NodeInfo> {
        if let Ok(Response::Nodes(nodes)) = self.client.send(Request::ListNodes).await {
            nodes
                .into_iter()
                .map(|(id, reg)| NodeInfo {
                    id,
                    address: reg.node_address,
                })
                .collect()
        } else {
            vec![]
        }
    }
}
