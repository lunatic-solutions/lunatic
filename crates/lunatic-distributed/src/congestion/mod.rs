use std::{
    io::IoSlice,
    sync::{
        atomic::{self, AtomicU64, AtomicUsize},
        Arc,
    },
};

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use tokio::{
    io::AsyncWriteExt,
    sync::{
        mpsc::{error::TryRecvError, Receiver, Sender},
        RwLock,
    },
};

use crate::{
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
    message_id: MessageId,
    env: EnvironmentId,
    src: ProcessId,
    stream: RwLock<quinn::SendStream>,
    chunk_id: AtomicU64,
    offset: AtomicUsize,
    data: Vec<u8>,
}

pub struct MessageChunk<'a> {
    message_id: u64,
    message_size: u32,
    chunk_id: u64,
    chunk_size: u32,
    data: &'a [u8],
}

type BufRx = RwLock<Receiver<MessageCtx>>;

// To replace distributed::Client
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

pub struct Inner {
    control_client: control::Client,
    node_client: quic::Client,
    next_message_id: AtomicU64,
    buf_rx: DashMap<EnvironmentId, DashMap<ProcessId, BufRx>>,
    buf_tx: DashMap<(EnvironmentId, ProcessId), Sender<MessageCtx>>,
    in_progress: DashMap<(EnvironmentId, ProcessId), MessageCtx>,
    connections: DashMap<NodeId, quinn::Connection>,
}

impl Client {
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
        // Lazy initialize remote node quic connection
        let conn = match self.inner.connections.get(&node) {
            Some(conn) => conn,
            None => {
                // Connect
                let node_info = self
                    .inner
                    .control_client
                    .node_info(node.0)
                    .ok_or_else(|| anyhow!("Node {} does not exist.", node.0))?;
                let conn = self
                    .inner
                    .node_client
                    .try_connect(node_info.address, &node_info.name, 3)
                    .await?;
                self.inner.connections.insert(node, conn);
                self.inner.connections.get(&node).unwrap()
            }
        };
        let (send, _) = conn.open_bi().await?; // TODO: RecvStream part

        match tx
            .send(MessageCtx {
                message_id: self.next_message_id(),
                env,
                src,
                stream: RwLock::new(send),
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
        self.new_message(params.env, params.src, params.node, data)
            .await
    }

    // Send distributed spawn message
    pub async fn spawn(&self, params: SpawnParams) -> Result<()> {
        let message = Request::Spawn(params.spawn);
        let data = match rmp_serde::to_vec(&message) {
            Ok(data) => data,
            Err(_) => unreachable!("lunatic::distributed::client::spawn serialize_message"),
        };
        self.new_message(params.env, params.src, params.node, data)
            .await
    }
}

// TODO: move to configuration
const CHUNK_SIZE: usize = 1024;

pub async fn congestion_control_worker(state: Client) -> ! {
    loop {
        for env in state.inner.buf_rx.iter() {
            let mut disconected = vec![];
            for pid in env.iter() {
                let key = (*env.key(), *pid.key());
                let finished = if let Some(msg_ctx) = state.inner.in_progress.get(&key) {
                    // Chunk data using offset
                    let offset = msg_ctx.offset.load(atomic::Ordering::Relaxed);
                    let (data, finished) = if msg_ctx.data.len() <= offset + CHUNK_SIZE {
                        // Chunk will be finished after this write
                        (&msg_ctx.data[offset..], true)
                    } else {
                        // Move to next Chunk
                        msg_ctx
                            .offset
                            .store(offset + CHUNK_SIZE, atomic::Ordering::Relaxed);
                        (&msg_ctx.data[offset..offset + CHUNK_SIZE], false)
                    };
                    // Create chunk
                    let chunk = MessageChunk {
                        message_id: msg_ctx.message_id.0,
                        message_size: msg_ctx.data.len() as u32,
                        chunk_id: msg_ctx.chunk_id.fetch_add(1, atomic::Ordering::Relaxed),
                        chunk_size: data.len() as u32,
                        data,
                    };
                    let mut stream = msg_ctx.stream.write().await;
                    // Serialize chunk to quic stream
                    let msg_id = chunk.message_id.to_le_bytes();
                    let msg_size = chunk.message_size.to_le_bytes();
                    let chunk_id = chunk.chunk_id.to_le_bytes();
                    let chunk_size = chunk.chunk_size.to_le_bytes();
                    let bufs = &[
                        IoSlice::new(&msg_id),
                        IoSlice::new(&msg_size),
                        IoSlice::new(&chunk_id),
                        IoSlice::new(&chunk_size),
                        IoSlice::new(chunk.data),
                    ];
                    stream.write_vectored(bufs).await.ok();
                    finished
                } else {
                    let mut recv = pid.write().await;
                    match recv.try_recv() {
                        // Push message into in progress space
                        Ok(new_msg_ctx) => {
                            state
                                .inner
                                .in_progress
                                .insert((new_msg_ctx.env, new_msg_ctx.src), new_msg_ctx);
                        }
                        // No new messages
                        Err(TryRecvError::Empty) => (),
                        // Process finished clean up
                        Err(TryRecvError::Disconnected) => {
                            disconected.push(*pid.key());
                        }
                    };
                    // Chunk in progress
                    false
                };
                if finished {
                    state.inner.in_progress.remove(&key);
                }
            }
            // remove disconnected processes
            for pid in disconected {
                env.remove(&pid);
            }
        }
    }
}
