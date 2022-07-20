use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use bytes::Bytes;
use dashmap::DashMap;
use lunatic_process::runtimes::RawWasm;
use s2n_quic::{
    client::Connect,
    stream::{ReceiveStream, SendStream},
    Client as QuicClient,
};
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc, RwLock},
    time::Duration,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
    connection::receive_message,
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
    node_name: String,
    control_addr: SocketAddr,
    tx: UnboundedSender<(u64, Request)>,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
    nodes: DashMap<u64, NodeInfo>,
    node_ids: RwLock<Vec<u64>>,
}

impl Client {
    pub async fn register(
        node_addr: SocketAddr,
        node_name: String,
        control_addr: SocketAddr,
        control_name: String,
        quic_client: QuicClient,
    ) -> Result<(u64, Self)> {
        let (tx, rx) = mpsc::unbounded_channel();

        let client = Client {
            inner: Arc::new(InnerClient {
                next_message_id: AtomicU64::new(1),
                control_addr,
                node_addr,
                node_name,
                tx,
                pending_requests: DashMap::new(),
                nodes: Default::default(),
                node_ids: Default::default(),
            }),
        };

        // Spawn connection task before register
        tokio::task::spawn(connection_task(
            client.clone(),
            quic_client,
            control_addr,
            control_name,
            rx,
        ));
        tokio::task::spawn(refresh_nodes_task(client.clone()));

        let node_id: u64 = client.send_registration().await?;
        Ok((node_id, client))
    }

    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn control_addr(&self) -> SocketAddr {
        self.inner.control_addr
    }

    pub async fn send(&self, req: Request) -> Result<Response> {
        let msg_id = self.next_message_id();
        self.inner.tx.send((msg_id, req))?;
        let cell = AsyncCell::shared();
        self.inner.pending_requests.insert(msg_id, cell.clone());
        let response = cell.take().await;
        self.inner.pending_requests.remove(&msg_id);
        Ok(response)
    }

    async fn send_registration(&self) -> Result<u64> {
        let reg = Registration {
            node_address: self.inner.node_addr,
            node_name: self.inner.node_name.clone(),
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

async fn refresh_nodes_task(client: Client) -> Result<()> {
    loop {
        client.refresh_nodes().await.ok();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Connection task receives data over Tokio MPSC channel and sends it to a Quic stream.
/// If Quic connection is closed, it tries to reconnect.
/// If the Tokio channel is closed (all senders dropped or manually closed), the task finishes
/// as we consider that the connection doesn't have to be kept alive anymore.
///
/// On every reconnect, a new reader task is spawned. The old reader task will finish when it detects
/// that the Quic write stream has been closed.
async fn connection_task(
    client: Client,
    quic_client: QuicClient,
    addr: SocketAddr,
    name: String,
    mut rx: UnboundedReceiver<(u64, Request)>,
) {
    let (mut conn, recv, mut send) = connect_try_forever(&quic_client, addr, &name).await;
    tokio::spawn(reader_task(client.clone(), recv));
    while let Some(msg) = rx.recv().await {
        println!("RECVD MSG {}: {}", msg.0, msg.1.kind());
        if let Ok(data) = bincode::serialize(&msg) {
            // Prefix message with size as little-endian u32 value.
            let size = (data.len() as u32).to_le_bytes();
            let size: Bytes = Bytes::copy_from_slice(&size[..]);

            // Bytes is used by s2n-quic, it's cheap to clone. We try to reconnect and re-send data
            // if it fails for any reason.
            let bytes: Bytes = data.into();
            println!("SEND MSG {}: {}", msg.0, msg.1.kind());
            while let Err(e) = send.send_vectored(&mut [size.clone(), bytes.clone()]).await {
                println!("ERROR SENDING MSG {}: {}", msg.0, msg.1.kind());
                // TODO: we should actually consider which stream error happened and decide what to do
                log::error!("Error sending data: {e}. Trying to reconnect to {addr} ({name}).");
                conn.close(s2n_quic::application::Error::UNKNOWN);
                let (new_conn, recv, new_send) =
                    connect_try_forever(&quic_client, addr, &name).await;
                tokio::spawn(reader_task(client.clone(), recv));
                conn = new_conn;
                send = new_send;
            }

            println!("SENT MSG {}: {}", msg.0, msg.1.kind());
        }
    }
}

async fn reader_task(client: Client, mut recv: ReceiveStream) {
    loop {
        if let Ok((id, resp)) = receive_message(&mut recv).await {
            client.process_response(id, resp);
        }
    }
}

async fn connect_try_forever(
    quic_client: &QuicClient,
    addr: SocketAddr,
    name: &str,
) -> (s2n_quic::Connection, ReceiveStream, SendStream) {
    loop {
        log::info!("Connecting to control {addr}");
        if let Ok(conn) = connect(quic_client, addr, name).await {
            return conn;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn connect(
    quic_client: &QuicClient,
    addr: SocketAddr,
    name: &str,
) -> Result<(s2n_quic::Connection, ReceiveStream, SendStream)> {
    let connect = Connect::new(addr).with_server_name(name);
    let mut conn = quic_client.connect(connect).await?;
    conn.keep_alive(true)?;
    let stream = conn.open_bidirectional_stream().await?;
    let (sender, receiver) = stream.split();
    Ok((conn, sender, receiver))
}
