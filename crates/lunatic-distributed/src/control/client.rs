use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use dashmap::DashMap;
use lunatic_process::runtimes::RawWasm;
use s2n_quic::{client::Connect, Client as QuicClient};
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc, RwLock},
    time::Duration,
};

use crate::{
    connection::Connection,
    control::{
        message::{Registered, Registration, Request, Response},
        server::CTRL_SERVER_NAME,
    },
    NodeInfo,
};

#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    next_message_id: AtomicU64,
    node_addr: SocketAddr,
    node_name: String,
    control_addr: SocketAddr,
    connection: Connection,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
    nodes: DashMap<u64, NodeInfo>,
    node_ids: RwLock<Vec<u64>>,
}

impl Client {
    pub async fn register(
        node_addr: SocketAddr,
        node_name: String,
        control_addr: SocketAddr,
        quic_client: QuicClient,
        signing_request: String,
    ) -> Result<(u64, Self, String)> {
        let client = Client {
            inner: Arc::new(InnerClient {
                next_message_id: AtomicU64::new(1),
                control_addr,
                node_addr,
                node_name,
                connection: connect(quic_client.clone(), control_addr, 5).await?,
                pending_requests: DashMap::new(),
                nodes: Default::default(),
                node_ids: Default::default(),
            }),
        };
        // Spawn reader task before register
        tokio::task::spawn(reader_task(client.clone()));
        tokio::task::spawn(refresh_nodes_task(client.clone()));
        let Registered {
            node_id,
            signed_cert,
        } = client.send_registration(signing_request).await?;
        Ok((node_id, client, signed_cert))
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

    async fn send_registration(&self, signing_request: String) -> Result<Registered> {
        let reg = Registration {
            node_address: self.inner.node_addr,
            node_name: self.inner.node_name.clone(),
            signing_request,
        };
        let resp = self.send(Request::Register(reg)).await?;
        match resp {
            Response::Register(data) => Ok(data),
            Response::Error(e) => Err(anyhow!("Registration failed. {e}")),
            _ => Err(anyhow!("Registration failed.")),
        }
    }

    fn process_response(&self, id: u64, resp: Response) {
        if let Some(e) = self.inner.pending_requests.get(&id) {
            e.set(resp);
        };
    }

    pub async fn refresh_nodes(&self) -> Result<()> {
        if let Response::Nodes(nodes) = self.send(Request::ListNodes).await? {
            let mut node_ids = vec![];
            for (id, reg) in nodes {
                node_ids.push(id);
                if !self.inner.nodes.contains_key(&id) {
                    self.inner.nodes.insert(
                        id,
                        NodeInfo {
                            id,
                            address: reg.node_address,
                            name: reg.node_name,
                        },
                    );
                }
            }
            if let Ok(mut self_node_ids) = self.inner.node_ids.write() {
                *self_node_ids = node_ids;
            }
        }
        Ok(())
    }

    pub fn node_info(&self, node_id: u64) -> Option<NodeInfo> {
        self.inner.nodes.get(&node_id).map(|e| e.clone())
    }

    pub fn node_ids(&self) -> Vec<u64> {
        self.inner.node_ids.read().unwrap().clone()
    }

    pub fn node_count(&self) -> usize {
        self.inner.node_ids.read().unwrap().len()
    }

    pub async fn get_module(&self, module_id: u64) -> Option<Vec<u8>> {
        if let Ok(Response::Module(module)) = self.send(Request::GetModule(module_id)).await {
            module
        } else {
            None
        }
    }

    pub async fn add_module(&self, module: Vec<u8>) -> Result<RawWasm> {
        if let Response::ModuleId(id) = self.send(Request::AddModule(module.clone())).await? {
            Ok(RawWasm::new(Some(id), module))
        } else {
            Err(anyhow::anyhow!("Invalid response type on add_module."))
        }
    }
}

async fn connect(quic_client: QuicClient, addr: SocketAddr, retry: u32) -> Result<Connection> {
    for _ in 0..retry {
        log::info!("Connecting to control {addr}");
        let connect = Connect::new(addr).with_server_name(CTRL_SERVER_NAME);
        if let Ok(mut conn) = quic_client.connect(connect).await {
            conn.keep_alive(true)?;
            let stream = conn.open_bidirectional_stream().await?;
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

async fn refresh_nodes_task(client: Client) -> Result<()> {
    loop {
        client.refresh_nodes().await.ok();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
