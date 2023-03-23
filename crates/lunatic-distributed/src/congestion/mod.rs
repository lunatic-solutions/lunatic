/// Congestion control module for distributed message chunking between competing processes.
///
/// When a process send out a message to another process on a different node
/// the message is forwarded to the Congestion control worker via a queue.
///
/// Congestion control worker picks up each message sent and routes chunks
/// to  node connection managers. For each node there exists only one connection
/// manager.
///
/// The node connection manager is responsible for routing message chunks based on
/// the source process_id and destination process_id to appropriate quic stream.
/// This ensures that all process to process messages come in order in which they
/// are being sent.
///
/// Stream task manages quic stream and writes multiple message chunks.
///
/// Topology illustration:
///
///  -----       -----
/// |  P  | ... |  P  | - Processes
///  -----       -----
///    |      /
///    |    /
///  -----
/// |  C  | - Congestion control worker
///  -----
///    | \
///    |   \
///    |     \
///    |       \
///  -----       -----
/// |  N  | ... |  N  | - Node connection managers
///  -----       -----
///    |         | \
///    |         ...
///    |
///    |
///  -----       -----
/// |  S  | ... |  S  | - Stream tasks
///  -----       -----
///
use std::{
    collections::VecDeque,
    sync::{atomic, Arc},
    time::Duration,
};

use anyhow::Result;
use lunatic_control::NodeInfo;
use tokio::sync::{
    mpsc::{self, error::TryRecvError, Receiver, Sender},
    RwLock,
};

use crate::{
    distributed::{self, client::ProcessId},
    quic,
};

pub struct MessageChunk {
    src: ProcessId,
    dest: ProcessId,
    message_id: u64,
    message_size: u32,
    chunk_id: u64,
    data: bytes::Bytes,
}

// TODO: move to configuration
const CHUNK_SIZE: usize = 1024;

pub async fn congestion_control_worker(state: distributed::Client) -> ! {
    log::trace!("starting congestion control worker");
    loop {
        let mut progress = false;
        for env in state.inner.buf_rx.iter() {
            let mut disconected = vec![];
            for pid in env.iter() {
                let key = (*env.key(), *pid.key());
                let finished = if let Some(msg_ctx) = state.inner.in_progress.get(&key) {
                    progress = true;
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
                                log::trace!(
                                    "congestion::chunk::sent message_id={} chunk_id={chunk_id}",
                                    msg_ctx.message_id.0
                                );
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
                            log::trace!(
                                "congestion::message::received message_id={}",
                                new_msg_ctx.message_id.0
                            );
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
        if !progress {
            tokio::time::sleep(Duration::from_millis(100)).await;
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
    log::trace!(
        "congestion::node_connection_manager::started node={} address={}",
        node_info.id,
        node_info.address
    );
    // Setup stream buffer
    let mut buffers: Vec<StreamBuffer> = Vec::with_capacity(manager.streams);
    for _ in 0..manager.streams {
        buffers.push(Arc::new(RwLock::new(VecDeque::new())));
    }
    // Setup stream dead waker
    let (dead_stream_notifier, mut dead_stream_waker) = mpsc::channel::<()>(1);

    loop {
        // Setup conn or fail
        let conn = match manager
            .client
            .try_connect(node_info.address, &node_info.name, 3)
            .await
        {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("congestion::node_connection_manager Connection failed: {e}");
                continue;
            }
        };
        log::trace!(
            "node={} name={} address={}",
            node_info.id,
            node_info.name,
            node_info.address
        );
        // Start stream tasks
        let mut stream_tasks = Vec::new();
        let mut stream_wakers = Vec::new();
        for buffer in buffers.iter().take(manager.streams) {
            let stream = match conn.open_uni().await {
                Ok(stream) => stream,
                Err(e) => {
                    log::error!("congestion::node_connection_manager Stream open failed: {e}");
                    continue;
                }
            };
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
                    log::trace!("congestion::node_connection_manager::recv_chunk {}", chunk.message_id);
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
    log::trace!("congestion::stream_task::start {}", state.quic_stream.id());
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
            Ok(_) => {
                log::trace!("congestion::stream_task::write");
            }
            Err(_) => {
                // Connection is dead return chunks in order back to the buffer
                chunks.drain(..).rev().for_each(|c| buffer.push_back(c));
                // Notify manager that connection has died
                state.manager_notifier.send(()).await.ok();
                break;
            }
        };
    }
    // state.manager_notifier.send(()).await.ok();
}
