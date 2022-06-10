use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use dashmap::DashMap;
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc},
    time::Duration,
};
use tokio::net::TcpStream;

use crate::{
    connection::Connection,
    control::message::{Registration, Request, Response},
    NodeInfo,
};

#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    next_message_id: AtomicU64,
    node_addr: SocketAddr,
    control_addr: SocketAddr,
    connection: Connection,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
}

impl Client {
    pub async fn register(node_addr: SocketAddr, control_addr: SocketAddr) -> Result<(u64, Self)> {
        let client = Client {
            inner: Arc::new(InnerClient {
                next_message_id: AtomicU64::new(1),
                control_addr,
                node_addr,
                connection: connect(control_addr, 5).await?,
                pending_requests: DashMap::new(),
            }),
        };
        // Spawn reader task before register
        tokio::task::spawn(reader_task(client.clone()));
        let node_id: u64 = client.send_registration().await?;
        Ok((node_id, client))
    }

    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn connection(&self) -> &Connection {
        &self.inner.connection
    }

    pub fn control_addr(&self) -> SocketAddr {
        self.inner.control_addr
    }

    pub async fn send(&self, req: Request) -> Result<Response> {
        let msg_id = self.next_message_id();
        self.inner.connection.send(msg_id, req).await?;
        let cell = AsyncCell::shared();
        self.inner.pending_requests.insert(msg_id, cell.clone());
        let response = cell.take().await;
        self.inner.pending_requests.remove(&msg_id);
        Ok(response)
    }

    pub async fn recv(&self) -> Result<(u64, Response)> {
        self.inner.connection.receive().await
    }

    async fn send_registration(&self) -> Result<u64> {
        let reg = Registration {
            node_address: self.inner.node_addr,
        };
        let resp = self.send(Request::Register(reg)).await?;
        if let Response::Register(node_id) = resp {
            return Ok(node_id);
        }
        Err(anyhow!("Registration failed."))
    }

    fn process_response(&self, id: u64, resp: Response) {
        if let Some(e) = self.inner.pending_requests.get(&id) {
            e.set(resp);
        };
    }

    pub async fn get_nodes(&self) -> Vec<NodeInfo> {
        if let Ok(Response::Nodes(nodes)) = self.send(Request::ListNodes).await {
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

    pub async fn get_module(&self, module_id: u64) -> Option<Vec<u8>> {
        if let Ok(Response::Module(module)) = self.send(Request::GetModule(module_id)).await {
            module
        } else {
            None
        }
    }

    pub async fn add_module(&self, module: Vec<u8>) -> Result<u64> {
        if let Response::ModuleId(id) = self.send(Request::AddModule(module)).await? {
            Ok(id)
        } else {
            Err(anyhow::anyhow!("Invalid response type on add_module."))
        }
    }
}

async fn connect(addr: SocketAddr, retry: u32) -> Result<Connection> {
    for _ in 0..retry {
        log::info!("Connecting to control {addr}");
        if let Ok(stream) = TcpStream::connect(addr).await {
            return Ok(Connection::new(stream));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(anyhow!("Failed to connect to {addr}"))
}

async fn reader_task(client: Client) -> Result<()> {
    loop {
        if let Ok((id, resp)) = client.recv().await {
            client.process_response(id, resp);
        }
    }
}
