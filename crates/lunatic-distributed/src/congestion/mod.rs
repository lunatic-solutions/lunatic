use std::{
    collections::VecDeque,
    sync::{
        atomic::{self, AtomicU64, AtomicUsize},
        Arc,
    },
};

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use tokio::sync::{
    mpsc::{self, error::TryRecvError, Receiver, Sender},
    RwLock,
};

use crate::{
    control,
    distributed::message::{Request, Spawn},
    quic, NodeInfo,
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
    node: NodeId,
    dest: ProcessId,
    chunk_id: AtomicU64,
    offset: AtomicUsize,
    data: Vec<u8>,
}

pub struct MessageChunk {
    src: ProcessId,
    dest: ProcessId,
    message_id: u64,
    message_size: u32,
    chunk_id: u64,
    data: bytes::Bytes,
}

// Receiving part of the message queue
type BufRx = RwLock<Receiver<MessageCtx>>;

// TODO: replace distributed::Client
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

pub struct Inner {
    control_client: control::Client,
    node_client: quic::Client,
    next_message_id: AtomicU64,
    // Across Environments and ProcessId's track message queues
    buf_rx: DashMap<EnvironmentId, DashMap<ProcessId, BufRx>>,
    // Sending part of the message queue
    buf_tx: DashMap<(EnvironmentId, ProcessId), Sender<MessageCtx>>,
    // Holds the message while its being chunked
    in_progress: DashMap<(EnvironmentId, ProcessId), MessageCtx>,
    nodes_queues: DashMap<NodeId, Sender<MessageChunk>>,
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
                .ok_or_else(|| anyhow!(""))?;
            let (send, recv) = tokio::sync::mpsc::channel(100);
            tokio::spawn(node_connection_manager(NodeConnectionManager {
                streams: 10,
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
    pub async fn spawn(&self, params: SpawnParams) -> Result<()> {
        let message = Request::Spawn(params.spawn);
        let data = match rmp_serde::to_vec(&message) {
            Ok(data) => data,
            Err(_) => unreachable!("lunatic::distributed::client::spawn serialize_message"),
        };
        self.new_message(params.env, params.src, params.node, ProcessId(0), data)
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
                    let chunk_id = msg_ctx.chunk_id.load(atomic::Ordering::Relaxed);
                    let (data, finished) = if msg_ctx.data.len() <= offset + CHUNK_SIZE {
                        // Chunk will be finished after this write
                        (bytes::Bytes::copy_from_slice(&msg_ctx.data[offset..]), true)
                    } else {
                        (
                            bytes::Bytes::copy_from_slice(
                                &msg_ctx.data[offset..offset + CHUNK_SIZE],
                            ),
                            false,
                        )
                    };
                    // Create chunk
                    let chunk = MessageChunk {
                        src: msg_ctx.src,
                        dest: msg_ctx.dest,
                        message_id: msg_ctx.message_id.0,
                        message_size: msg_ctx.data.len() as u32,
                        chunk_id,
                        data,
                    };
                    if let Some(node_queue) = state.inner.nodes_queues.get(&msg_ctx.node) {
                        match node_queue.try_send(chunk) {
                            Ok(_) => {
                                // Move to next chunk
                                msg_ctx
                                    .offset
                                    .store(offset + CHUNK_SIZE, atomic::Ordering::Relaxed);
                                msg_ctx
                                    .chunk_id
                                    .store(chunk_id + 1, atomic::Ordering::Relaxed);
                                finished
                            }
                            Err(e) => {
                                log::warn!(
                                    "Cannot send next chunk from pid={} to node={} dest_pid={}, reason: {e}",
                                    msg_ctx.src.0,
                                    msg_ctx.node.0,
                                    msg_ctx.dest.0,
                                );
                                finished
                            }
                        }
                    } else {
                        log::error!("Connection to node={} does not exist", msg_ctx.node.0);
                        false
                    }
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

type StreamBuffer = Arc<RwLock<VecDeque<MessageChunk>>>;

enum StreamAction {
    Message,
    Die,
}

pub struct NodeConnectionManager {
    pub streams: usize,
    pub node_info: NodeInfo,
    pub client: quic::Client,
    pub message_chunks: Receiver<MessageChunk>,
}

pub async fn node_connection_manager(mut manager: NodeConnectionManager) -> Result<()> {
    let node_info = manager.node_info;
    // Setup stream buffer
    let mut buffers: Vec<StreamBuffer> = Vec::with_capacity(manager.streams);
    for _ in 0..manager.streams {
        buffers.push(Arc::new(RwLock::new(VecDeque::new())));
    }
    // Setup stream dead waker
    let (dead_stream_notifier, mut dead_stream_waker) = mpsc::channel::<()>(1);

    loop {
        // Setup conn or fail
        let conn = manager
            .client
            .try_connect(node_info.address, &node_info.name, 3)
            .await?;
        // Start stream tasks
        let mut stream_tasks = Vec::new();
        let mut stream_wakers = Vec::new();
        for buffer in buffers.iter().take(manager.streams) {
            let stream = conn.open_uni().await?;
            let (send, recv) = mpsc::channel::<StreamAction>(100);
            stream_wakers.push(send);
            stream_tasks.push(tokio::spawn(stream_task(StreamTask {
                quic_stream: stream,
                action: recv,
                manager_notifier: dead_stream_notifier.clone(),
                buffer: buffer.clone(),
            })));
        }
        // Working chunk passing loop
        'forward_chunks: loop {
            tokio::select! {
                Some(chunk) = manager.message_chunks.recv() => {
                    let src = chunk.src.0;
                    let dest = chunk.dest.0;
                    // Determine stream index by source and destination process_id
                    // This ensures that all messages arrive in order between processes
                    let stream_index = ((src ^ dest) % manager.streams as u64) as usize;
                    // Push data into message buffer
                    buffers[stream_index].write().await.push_front(chunk);
                    // Wake up stream task
                    stream_wakers[stream_index].try_send(StreamAction::Message).ok();
                },
                _ = dead_stream_waker.recv() => {
                    break 'forward_chunks;
                },
            };
        }
        // Try to wake up all remaining streams
        for stream in stream_wakers {
            stream.try_send(StreamAction::Die).ok();
        }
        // Clean up tasks
        for task in stream_tasks {
            task.await.ok();
        }
    }
}

struct StreamTask {
    quic_stream: quinn::SendStream,
    action: Receiver<StreamAction>,
    manager_notifier: Sender<()>,
    buffer: StreamBuffer,
}

async fn stream_task(mut state: StreamTask) {
    while let Some(StreamAction::Message) = state.action.recv().await {
        let mut buffer = state.buffer.write().await;
        let mut chunks = Vec::new();
        while let Some(chunk) = buffer.pop_back() {
            chunks.push(chunk);
        }
        let mut data: Vec<bytes::Bytes> = chunks
            .iter()
            .map(|c| {
                let mut buf = Vec::new();
                buf.extend(c.message_id.to_le_bytes().as_ref());
                buf.extend(c.message_size.to_le_bytes().as_ref());
                buf.extend(c.chunk_id.to_le_bytes().as_ref());
                buf.extend((c.data.len() as u32).to_le_bytes().as_ref());
                buf.extend(&c.data);
                bytes::Bytes::from(buf)
            })
            .collect();
        // Try to send data
        match state.quic_stream.write_all_chunks(&mut data).await {
            Ok(_) => (),
            Err(_) => {
                // Connection is dead return chunks in order back to the buffer
                chunks.drain(..).rev().for_each(|c| buffer.push_back(c));
                // Notify manager that connection has died
                state.manager_notifier.send(()).await.ok();
                break;
            }
        };
    }
}
