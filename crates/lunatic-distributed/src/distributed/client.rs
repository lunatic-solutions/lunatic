use std::sync::{
    atomic::{self, AtomicU64, AtomicUsize},
    Arc,
};

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    RwLock,
};

use crate::{
    congestion::{node_connection_manager, MessageChunk, NodeConnectionManager},
    control,
    distributed::message::{Request, Spawn},
    quic,
};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct EnvironmentId(pub u64);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct ProcessId(pub u64);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct NodeId(pub u64);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct MessageId(pub u64);

pub struct SendParams {
    pub env: EnvironmentId,
    pub src: ProcessId,
    pub node: NodeId,
    pub dest: ProcessId,
    pub tag: Option<i64>,
    pub data: Vec<u8>,
}

pub struct SpawnParams {
    pub env: EnvironmentId,
    pub src: ProcessId,
    pub node: NodeId,
    pub spawn: Spawn,
}

pub struct MessageCtx {
    pub message_id: MessageId,
    pub env: EnvironmentId,
    pub src: ProcessId,
    pub node: NodeId,
    pub dest: ProcessId,
    pub chunk_id: AtomicU64,
    pub offset: AtomicUsize,
    pub data: Vec<u8>,
}

// Receiving part of the message queue
type BufRx = RwLock<Receiver<MessageCtx>>;

// TODO: replace distributed::Client
#[derive(Clone)]
pub struct Client {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    control_client: control::Client,
    node_client: quic::Client,
    pub next_message_id: AtomicU64,
    // Across Environments and ProcessId's track message queues
    pub buf_rx: DashMap<EnvironmentId, DashMap<ProcessId, BufRx>>,
    // Sending part of the message queue
    pub buf_tx: DashMap<(EnvironmentId, ProcessId), Sender<MessageCtx>>,
    // Holds the message while its being chunked
    pub in_progress: DashMap<(EnvironmentId, ProcessId), MessageCtx>,
    pub nodes_queues: DashMap<NodeId, Sender<MessageChunk>>,
}

impl Client {
    pub fn new(control_client: control::Client, node_client: quic::Client) -> Self {
        Self {
            inner: Arc::new(Inner {
                control_client,
                node_client,
                next_message_id: AtomicU64::new(1),
                buf_rx: DashMap::new(),
                buf_tx: DashMap::new(),
                in_progress: DashMap::new(),
                nodes_queues: DashMap::new(),
            }),
        }
    }

    fn next_message_id(&self) -> MessageId {
        MessageId(
            self.inner
                .next_message_id
                .fetch_add(1, atomic::Ordering::Relaxed),
        )
    }

    async fn new_message(
        &self,
        env: EnvironmentId,
        src: ProcessId,
        node: NodeId,
        dest: ProcessId,
        data: Vec<u8>,
    ) -> Result<()> {
        // Lazy initialize process message buffers
        let tx = match self.inner.buf_tx.get(&(env, src)) {
            Some(tx) => tx,
            None => {
                let (send, recv) = tokio::sync::mpsc::channel(100); // TODO: configuration
                match self.inner.buf_rx.get(&env) {
                    Some(env_queue) => {
                        env_queue.insert(src, RwLock::new(recv));
                    }
                    None => {
                        let queue = DashMap::new();
                        queue.insert(src, RwLock::new(recv));
                        self.inner.buf_rx.insert(env, queue);
                    }
                };
                self.inner.buf_tx.insert((env, src), send);
                self.inner.buf_tx.get(&(env, src)).unwrap()
            }
        };

        let node_manager_exists = self.inner.nodes_queues.get(&node).is_none();

        if node_manager_exists {
            let node_info = self
                .inner
                .control_client
                .node_info(node.0)
                .ok_or_else(|| anyhow!("Node does not exist"))?;
            let (send, recv) = tokio::sync::mpsc::channel(100); // TODO: configuration
            tokio::spawn(node_connection_manager(NodeConnectionManager {
                streams: 10, // TODO: configuration
                node_info,
                client: self.inner.node_client.clone(),
                message_chunks: recv,
            }));
            self.inner.nodes_queues.insert(node, send);
        }

        match tx
            .send(MessageCtx {
                message_id: self.next_message_id(),
                env,
                src,
                node,
                dest,
                offset: AtomicUsize::new(0),
                chunk_id: AtomicU64::new(0),
                data,
            })
            .await
        {
            Ok(_) => (),
            Err(_) => log::error!("lunatic::distributed::client::send"),
        };
        Ok(())
    }

    // TODO: how to detect process is dead?
    pub fn remove_process_resources(&self, env: EnvironmentId, process_id: ProcessId) {
        self.inner.buf_tx.remove(&(env, process_id));
    }

    // Send distributed message
    pub async fn send(&self, params: SendParams) -> Result<()> {
        let message = Request::Message {
            environment_id: params.env.0,
            process_id: params.dest.0,
            tag: params.tag,
            data: params.data,
        };
        let data = match rmp_serde::to_vec(&message) {
            Ok(data) => data,
            Err(_) => unreachable!("lunatic::distributed::client::send serialize_message"),
        };
        self.new_message(params.env, params.src, params.node, params.dest, data)
            .await
    }

    // Send distributed spawn message
    pub async fn spawn(&self, params: SpawnParams) -> Result<u64> {
        let message = Request::Spawn(params.spawn);
        let data = match rmp_serde::to_vec(&message) {
            Ok(data) => data,
            Err(_) => unreachable!("lunatic::distributed::client::spawn serialize_message"),
        };
        self.new_message(params.env, params.src, params.node, ProcessId(0), data)
            .await?;
        todo!()
    }
}
