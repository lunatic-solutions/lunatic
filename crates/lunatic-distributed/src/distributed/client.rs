use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use dashmap::DashMap;
use log;
use s2n_quic::{client::Connect, Client as QuicClient};
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc},
    time::Duration,
};

use crate::{
    connection::Connection,
    control,
    distributed::message::{Request, Response},
    NodeInfo,
};

use super::message::Val;

#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    next_message_id: AtomicU64,
    node_connections: DashMap<u64, Connection>,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
    control_client: control::Client,
    quic_client: QuicClient,
}

impl Client {
    // TODO node_id?
    pub async fn new(
        _node_id: u64,
        control_client: control::Client,
        quic_client: QuicClient,
    ) -> Result<Client> {
        let client = Client {
            inner: Arc::new(InnerClient {
                next_message_id: AtomicU64::new(1),
                node_connections: DashMap::new(),
                pending_requests: DashMap::new(),
                control_client,
                quic_client,
            }),
        };

        Ok(client)
    }

    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub async fn connection(&self, node_id: u64) -> Option<Connection> {
        match self.inner.node_connections.get(&node_id).map(|e| e.clone()) {
            Some(c) => Some(c),
            None => {
                let node_info = self.inner.control_client.node_info(node_id);

                let node_info = if node_info.is_none() {
                    self.inner.control_client.refresh_nodes().await.ok();
                    self.inner.control_client.node_info(node_id)
                } else {
                    node_info
                };

                match node_info {
                    Some(node) => {
                        if let Ok(conn) =
                            connect(self.inner.quic_client.clone(), node.address, &node.name, 2)
                                .await
                        {
                            self.inner.node_connections.insert(node.id, conn.clone());
                            tokio::task::spawn(reader_task(self.clone(), conn.clone()));
                            Some(conn)
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
        }
    }

    pub async fn connect(&self, node: &NodeInfo) {
        if let Ok(connection) =
            connect(self.inner.quic_client.clone(), node.address, &node.name, 3).await
        {
            self.inner.node_connections.insert(node.id, connection);
        }
    }

    pub async fn request(&self, node_id: u64, req: Request) -> Result<Response> {
        let msg_id = self.next_message_id();
        self.connection(node_id)
            .await
            .ok_or_else(|| anyhow!("No connection to node {node_id}"))?
            .send(msg_id, req)
            .await?;
        let cell = AsyncCell::shared();
        self.inner.pending_requests.insert(msg_id, cell.clone());
        let response = cell.take().await;
        self.inner.pending_requests.remove(&msg_id);
        Ok(response)
    }

    pub async fn send(&self, node_id: u64, req: Request) -> Result<u64> {
        let msg_id = self.next_message_id();
        self.connection(node_id)
            .await
            .ok_or_else(|| anyhow!("No connection to node {node_id}"))?
            .send(msg_id, req)
            .await?;
        Ok(msg_id)
    }

    pub async fn message_process(
        &self,
        node_id: u64,
        environment_id: u64,
        process_id: u64,
        tag: Option<i64>,
        data: Vec<u8>,
    ) -> Result<()> {
        self.send(
            node_id,
            Request::Message {
                environment_id,
                process_id,
                tag,
                data,
            },
        )
        .await?;
        Ok(())
    }

    fn process_response(&self, id: u64, resp: Response) {
        if let Some(e) = self.inner.pending_requests.get(&id) {
            e.set(resp);
        };
    }

    pub async fn spawn(
        &self,
        environment_id: u64,
        node_id: u64,
        module_id: u64,
        function: &str,
        params: Vec<Val>,
    ) -> Result<u64> {
        if let Response::Spawned(id) = self
            .request(
                node_id,
                Request::Spawn {
                    environment_id,
                    module_id,
                    function: function.into(),
                    params,
                },
            )
            .await?
        {
            Ok(id)
        } else {
            Err(anyhow!("Invalid response type for spawn"))
        }
    }
}

async fn connect(
    quic_client: QuicClient,
    addr: SocketAddr,
    server_name: &str,
    retry: u32,
) -> Result<Connection> {
    for _ in 0..retry {
        log::info!("Connecting to node on {addr}");
        let connect = Connect::new(addr).with_server_name(server_name);
        if let Ok(mut conn) = quic_client.connect(connect).await {
            conn.keep_alive(true)?;
            let stream = conn.open_bidirectional_stream().await?;
            return Ok(Connection::new(stream));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(anyhow!("Failed to connect to {addr}"))
}

async fn reader_task(client: Client, node_connection: Connection) -> Result<()> {
    loop {
        if let Ok((id, resp)) = node_connection.receive().await {
            client.process_response(id, resp);
        }
    }
}
