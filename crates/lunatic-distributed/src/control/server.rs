use std::{
    net::SocketAddr,
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use anyhow::Result;

use dashmap::DashMap;

use crate::{
    connection::new_quic_server,
    control::message::{Request, Response},
};
use crate::{connection::Connection, control::message::Registration};

#[derive(Clone)]
pub struct Server {
    inner: Arc<InnerServer>,
}

struct InnerServer {
    next_node_id: AtomicU64,
    nodes: DashMap<u64, Registration>,
    next_module_id: AtomicU64,
    modules: DashMap<u64, Vec<u8>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerServer {
                next_node_id: AtomicU64::new(1),
                next_module_id: AtomicU64::new(1),
                nodes: DashMap::new(),
                modules: DashMap::new(),
            }),
        }
    }

    pub fn next_node_id(&self) -> u64 {
        self.inner
            .next_node_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn next_module_id(&self) -> u64 {
        self.inner
            .next_module_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    fn register(&self, reg: Registration) -> Response {
        let node_id = self.next_node_id();
        self.inner.nodes.insert(node_id, reg);
        Response::Register(node_id)
    }

    fn list_nodes(&self) -> Response {
        Response::Nodes(
            self.inner
                .nodes
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect(),
        )
    }

    fn add_module(&self, bytes: Vec<u8>) -> Response {
        let module_id = self.next_module_id();
        self.inner.modules.insert(module_id, bytes);
        Response::ModuleId(module_id)
    }

    fn get_module(&self, id: u64) -> Response {
        Response::Module(self.inner.modules.get(&id).map(|e| e.clone()))
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn control_server(socket: SocketAddr, cert: String, key: String) -> Result<()> {
    let mut quic_server = new_quic_server(socket, cert, key)?;
    log::info!("Control server listening on {socket}");
    let server = Server::new();
    while let Some(conn) = quic_server.accept().await {
        let addr = conn.remote_addr().unwrap();
        log::info!("New connection {addr}");
        tokio::task::spawn(handle_quic_connection(server.clone(), conn));
    }
    Ok(())
}

async fn handle_quic_connection(server: Server, mut conn: s2n_quic::Connection) {
    while let Ok(Some(stream)) = conn.accept_bidirectional_stream().await {
        tokio::spawn(handle_quic_stream(server.clone(), Connection::new(stream)));
    }
}

async fn handle_quic_stream(server: Server, conn: Connection) {
    while let Ok((msg_id, request)) = conn.receive::<Request>().await {
        tokio::spawn(handle_request(
            server.clone(),
            conn.clone(),
            msg_id,
            request,
        ));
    }
}

async fn handle_request(
    server: Server,
    conn: Connection,
    msg_id: u64,
    request: Request,
) -> Result<u64> {
    println!("HANDLE REQUEST {msg_id}: {}", request.kind());
    use crate::control::message::Request::*;
    let response = match request {
        Register(reg) => server.register(reg),
        ListNodes => server.list_nodes(),
        AddModule(bytes) => server.add_module(bytes),
        GetModule(id) => server.get_module(id),
    };
    conn.send(msg_id, response).await
}
