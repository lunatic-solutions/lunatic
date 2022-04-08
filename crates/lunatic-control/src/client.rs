use anyhow::{anyhow, Result};
use async_cell::sync::AsyncCell;
use dashmap::DashMap;
use std::{
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc},
    time::Duration,
};

use async_std::channel::Receiver;
use async_std::{net::TcpStream, task};

use crate::{
    connection::Connection,
    message::{Registration, Request, Response},
};
use lunatic_common_api::{
    actor::{self, Actor, Responder},
    control::{ControlInterface, GetModule, GetNodeIds, RegisterModule},
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

pub async fn register(node_addr: SocketAddr, control_addr: SocketAddr) -> Result<ControlInterface> {
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
    task::spawn(reader_task(client.clone()));
    let node_id: u64 = client.register().await?;
    let control = ControlInterface {
        node_id,
        get_module: client.clone().spawn(),
        get_nodes: client.clone().spawn(),
        register_module: client.clone().spawn(),
    };
    Ok(control)
}

impl Client {
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

    async fn register(&self) -> Result<u64> {
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

async fn reader_task(client: Client) -> Result<()> {
    loop {
        if let Ok((id, resp)) = client.recv().await {
            client.process_response(id, resp);
        }
    }
}
trait ConvertRequest: actor::Request {
    fn into_ctrl_request(self) -> Request;
    fn from_ctrl_response(resp: Response) -> Option<Self::Response>;
}

// Implement the same actor for all control interface messages
// It converts actor requests into control server requests and waits for the response from the server
impl<T: ConvertRequest + actor::Request + Sync + Send + 'static> Actor<T> for Client {
    fn spawn_task(self, receiver: Receiver<(T, Responder<T>)>) {
        task::spawn(async move {
            while let Ok((req, resp)) = receiver.recv().await {
                let client = self.clone();
                task::spawn(async move {
                    if let Ok(r) = client.send(req.into_ctrl_request()).await {
                        if let Some(r) = T::from_ctrl_response(r) {
                            resp.respond(r).await;
                        }
                    };
                });
            }
        });
    }
}

impl ConvertRequest for GetNodeIds {
    fn into_ctrl_request(self) -> Request {
        Request::ListNodes
    }

    fn from_ctrl_response(resp: Response) -> Option<Self::Response> {
        if let Response::Nodes(nodes) = resp {
            Some(nodes)
        } else {
            None
        }
    }
}

impl ConvertRequest for GetModule {
    fn into_ctrl_request(self) -> Request {
        Request::GetModule(self.module_id)
    }

    fn from_ctrl_response(resp: Response) -> Option<Self::Response> {
        if let Response::Module(bytes) = resp {
            Some(bytes)
        } else {
            None
        }
    }
}

impl ConvertRequest for RegisterModule {
    fn into_ctrl_request(self) -> Request {
        Request::RegisterModule(self.bytes)
    }

    fn from_ctrl_response(resp: Response) -> Option<Self::Response> {
        if let Response::RegisterModule(id) = resp {
            Some(id)
        } else {
            None
        }
    }
}
