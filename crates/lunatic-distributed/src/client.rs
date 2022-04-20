use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use dashmap::DashMap;
use log;
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc},
    time::Duration,
};

use async_std::{channel::Receiver, net::TcpStream, task};

use crate::{
    connection::Connection,
    message::{Request, Response},
};

use lunatic_common_api::{
    actor::{self, Actor, Responder},
    control::{ControlInterface, NodeInfo},
    distributed::{DistributedInterface, Spawn},
};

#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    next_message_id: AtomicU64,
    node_connections: DashMap<u64, Connection>,
    node_info: DashMap<u64, NodeInfo>,
    pending_requests: DashMap<u64, Arc<AsyncCell<Response>>>,
    control: ControlInterface,
}

pub async fn start_client(control: ControlInterface) -> Result<DistributedInterface> {
    let client = Client {
        inner: Arc::new(InnerClient {
            next_message_id: AtomicU64::new(1),
            node_connections: DashMap::new(),
            node_info: DashMap::new(),
            pending_requests: DashMap::new(),
            control,
        }),
    };

    let nodes = client
        .inner
        .control
        .get_nodes
        .call(lunatic_common_api::control::GetNodes {})
        .await;

    log::info!("List nodes {nodes:?}");

    for node in nodes {
        let id = node.id;
        client.inner.node_info.insert(node.id, node);

        if id != client.inner.control.node_id {
            let resp = client.send(id, Request::Spawn).await;
            log::info!("Response {resp:?}")
        }
    }

    Ok(DistributedInterface {
        spawn: client.spawn(),
    })
}

impl Client {
    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub async fn connection(&self, node_id: u64) -> Option<Connection> {
        match self.inner.node_connections.get(&node_id).map(|e| e.clone()) {
            Some(c) => Some(c),
            None => match self.inner.node_info.get(&node_id) {
                Some(node) => {
                    if let Ok(conn) = connect(node.address, 2).await {
                        self.inner.node_connections.insert(node.id, conn.clone());
                        task::spawn(reader_task(self.clone(), conn.clone()));
                        Some(conn)
                    } else {
                        None
                    }
                }
                None => None,
            },
        }
    }

    pub async fn connect(&self, node: &NodeInfo) {
        if let Ok(stream) = TcpStream::connect(node.address).await {
            self.inner
                .node_connections
                .insert(node.id, Connection::new(stream));
        }
    }

    pub async fn send(&self, node_id: u64, req: Request) -> Result<Response> {
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

    fn process_response(&self, id: u64, resp: Response) {
        if let Some(e) = self.inner.pending_requests.get(&id) {
            e.set(resp);
        };
    }
}

async fn connect(addr: SocketAddr, retry: u32) -> Result<Connection> {
    for _ in 0..retry {
        log::info!("Connecting to control {addr}");
        if let Ok(stream) = TcpStream::connect(addr).await {
            return Ok(Connection::new(stream));
        }
        task::sleep(Duration::from_secs(2)).await;
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
trait ConvertRequest: actor::Request {
    fn into_ctrl_request(self) -> (u64, Request);
    fn from_ctrl_response(resp: Response) -> Option<Self::Response>;
}

impl ConvertRequest for Spawn {
    fn into_ctrl_request(self) -> (u64, Request) {
        (self.node_id, Request::Spawn)
    }

    fn from_ctrl_response(resp: Response) -> Option<Self::Response> {
        if let Response::Spawned = resp {
            Some(0) // TODO
        } else {
            None
        }
    }
}

// Implement the same actor for all control interface messages
// It converts actor requests into control server requests and waits for the response from the server
impl<T: ConvertRequest + actor::Request + Sync + Send + 'static> Actor<T> for Client {
    fn spawn_task(self, receiver: Receiver<(T, Responder<T>)>) {
        task::spawn(async move {
            while let Ok((req, resp)) = receiver.recv().await {
                let client = self.clone();
                task::spawn(async move {
                    let (node_id, req) = req.into_ctrl_request();
                    if let Ok(r) = client.send(node_id, req).await {
                        if let Some(r) = T::from_ctrl_response(r) {
                            resp.respond(r).await;
                        }
                    };
                });
            }
        });
    }
}
