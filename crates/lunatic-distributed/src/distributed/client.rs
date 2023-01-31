use anyhow::Result;
use async_cell::sync::AsyncCell;
use bytes::Bytes;
use dashmap::DashMap;
use std::sync::{atomic, atomic::AtomicU64, Arc};
use tokio::sync::mpsc::{self, unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::{
    control,
    distributed::message::{ClientError, Request, Response},
    quic::{self, RecvStream},
    NodeInfo,
};

use super::message::Spawn;

struct SendRequest {
    msg_id: u64,
    node_id: u64,
    request: Request,
}
#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    next_message_id: AtomicU64,
    node_message_buffers: DashMap<u64, UnboundedSender<(u64, Request)>>,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
    control_client: control::Client,
    quic_client: quic::Client,
    tx: UnboundedSender<SendRequest>,
}

impl Client {
    // TODO node_id?
    pub async fn new(
        _node_id: u64,
        control_client: control::Client,
        quic_client: quic::Client,
    ) -> Result<Client> {
        let (tx, rx) = mpsc::unbounded_channel();
        let client = Client {
            inner: Arc::new(InnerClient {
                next_message_id: AtomicU64::new(1),
                node_message_buffers: DashMap::new(),
                pending_requests: DashMap::new(),
                control_client,
                quic_client,
                tx,
            }),
        };
        tokio::spawn(forward_node_messages(client.clone(), rx));
        Ok(client)
    }

    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    async fn request(&self, node_id: u64, request: Request) -> Result<Response, ClientError> {
        let msg_id = self.next_message_id();
        self.inner
            .tx
            .send(SendRequest {
                msg_id,
                node_id,
                request,
            })
            .map_err(|e| ClientError::Unexpected(e.to_string()))?;
        let cell = AsyncCell::shared();
        self.inner.pending_requests.insert(msg_id, cell.clone());
        let response = cell.take().await;
        self.inner.pending_requests.remove(&msg_id);
        Ok(response)
    }

    pub async fn message_process(
        &self,
        node_id: u64,
        environment_id: u64,
        process_id: u64,
        tag: Option<i64>,
        data: Vec<u8>,
    ) -> Result<(), ClientError> {
        match self
            .request(
                node_id,
                Request::Message {
                    environment_id,
                    process_id,
                    tag,
                    data,
                },
            )
            .await
        {
            Ok(Response::Sent) => Ok(()),
            Ok(Response::Error(error)) | Err(error) => Err(error),
            Ok(_) => Err(ClientError::Unexpected(
                "Invalid response type for send".to_string(),
            )),
        }
    }

    fn process_response(&self, id: u64, resp: Response) {
        if let Some(e) = self.inner.pending_requests.get(&id) {
            e.set(resp);
        };
    }

    pub async fn spawn(&self, node_id: u64, spawn: Spawn) -> Result<u64, ClientError> {
        match self.request(node_id, Request::Spawn(spawn)).await {
            Ok(Response::Spawned(id)) => Ok(id),
            Ok(Response::Error(error)) | Err(error) => Err(error),
            Ok(_) => Err(ClientError::Unexpected(
                "Invalid response type for spawn".to_string(),
            )),
        }
    }
}

async fn reader_task(client: Client, mut recv: RecvStream) -> Result<()> {
    loop {
        match recv.receive().await {
            Ok(bytes) => {
                let (msg_id, response) =
                    rmp_serde::from_slice::<(u64, super::message::Response)>(&bytes)?;
                client.process_response(msg_id, response);
                Ok(())
            }
            Err(e) => {
                log::debug!("Node connection error: {e}");
                Err(e)
            }
        }?;
    }
}

async fn forward_node_messages(client: Client, mut rx: UnboundedReceiver<SendRequest>) {
    while let Some(SendRequest {
        msg_id,
        node_id,
        request,
    }) = rx.recv().await
    {
        if let Some(node_buf) = client.inner.node_message_buffers.get(&node_id) {
            node_buf.value().send((msg_id, request)).ok();
        } else {
            let (send, recv) = unbounded_channel();
            send.send((msg_id, request)).ok();
            client.inner.node_message_buffers.insert(node_id, send);
            tokio::spawn(manage_node_connection(node_id, client.clone(), recv));
        }
    }
}

async fn try_node_info_forever(node_id: u64, client: &Client) -> NodeInfo {
    loop {
        let node_info = client.inner.control_client.node_info(node_id);
        if node_info.is_none() {
            client.inner.control_client.refresh_nodes().await.ok();
        } else {
            return node_info.unwrap();
        }
    }
}

async fn manage_node_connection(
    node_id: u64,
    client: Client,
    mut rx: UnboundedReceiver<(u64, Request)>,
) {
    let quic_client = client.inner.quic_client.clone();
    let NodeInfo { address, name, .. } = try_node_info_forever(node_id, &client).await;
    let (mut send, recv) = quic::try_connect_forever(&quic_client, address, &name).await;
    tokio::spawn(reader_task(client.clone(), recv));
    while let Some(msg) = rx.recv().await {
        if let Ok(data) = rmp_serde::to_vec(&msg) {
            let size = (data.len() as u32).to_le_bytes();
            let size: Bytes = Bytes::copy_from_slice(&size[..]);
            let bytes: Bytes = data.into();
            while let Err(e) = send.send(&mut [size.clone(), bytes.clone()]).await {
                log::debug!("Cannot send data to node: {e}, reconnecting...");
                let (new_send, new_recv) =
                    quic::try_connect_forever(&quic_client, address, &name).await;
                tokio::spawn(reader_task(client.clone(), new_recv));
                send = new_send;
            }
        }
    }
}
