use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Result};
use async_std::{
    channel::{unbounded, Receiver, Sender},
    io::{ReadExt, WriteExt},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{Mutex, RwLock},
};
use bincode::{deserialize, serialize};
use log::trace;
use serde::{Deserialize, Serialize};

use crate::{module::Module, state::HashMapId, EnvConfig, Environment, Process};

/// A node holds information of other peers in the distributed system and resources belonging to
/// these remote nodes.
#[derive(Clone)]
pub struct Node {
    inner: Arc<RwLock<InnerNode>>,
}

struct InnerNode {
    name: String,
    socket: SocketAddr,
    peers: HashMap<String, Peer>,
    resources: HashMapId<Resource>,
}

impl Node {
    pub async fn new<A: ToSocketAddrs>(
        name: String,
        socket: A,
        bootstrap: Option<A>,
    ) -> Result<Node> {
        let socket = socket.to_socket_addrs().await?.next().unwrap();
        // Bind itself to a socket
        let listener = TcpListener::bind(socket).await?;

        // Bootstrap all peers from one.
        let peers = if let Some(bootstrap) = bootstrap {
            let bootstrap = bootstrap.to_socket_addrs().await?.next().unwrap();
            let bootstrap_conn = TcpStream::connect(bootstrap).await?;
            let mut bootstrap_peer = Peer {
                conn: bootstrap_conn,
                addr: bootstrap,
                response: unbounded(),
                send_mutex: Arc::new(Mutex::new(())),
            };
            // Register ourself at the bootstrap peer
            bootstrap_peer
                .send(Message::Register(name.clone(), socket.into()))
                .await?;
            // Ask the bootstrap for all other peers. The returned list will also contain the
            // bootstrap one.
            bootstrap_peer.send(Message::GetPeers).await?;
            if let Message::Peers(peer_infos) = bootstrap_peer.receive().await? {
                let mut peers = HashMap::new();
                for (peer_name, peer_addr) in peer_infos.into_iter() {
                    // At this point we are also a peer of the bootstrap node and we want to skip
                    // over ourself.
                    if socket == peer_addr {
                        continue;
                    }

                    let peer_conn = TcpStream::connect(peer_addr).await?;
                    let mut peer = Peer {
                        conn: peer_conn,
                        addr: peer_addr,
                        response: unbounded(),
                        send_mutex: Arc::new(Mutex::new(())),
                    };
                    // Register ourself
                    peer.send(Message::Register(name.clone(), socket)).await?;
                    peers.insert(peer_name, peer);
                }
                peers
            } else {
                return Err(anyhow!("Unexpected message from bootstrap node"));
            }
            // The `bootstrap_peer` is dropped here, but we still have a connection to it that we
            // established when receiving it from the `GetPeers` request.
        } else {
            HashMap::new()
        };

        let inner = InnerNode {
            name,
            socket,
            peers,
            resources: HashMapId::new(),
        };
        let node = Node {
            inner: Arc::new(RwLock::new(inner)),
        };

        {
            let node_read = node.inner.read().await;
            // Handle incoming messages of connected peers.
            for (_, peer) in node_read.peers.iter() {
                async_std::task::spawn(peer_task(node.clone(), peer.clone()));
            }
        }

        // Listen for new incoming connections.
        async_std::task::spawn(node_server(node.clone(), listener));

        Ok(node)
    }

    pub async fn peers(&self) -> HashMap<String, Peer> {
        let node = self.inner.read().await;
        node.peers.clone()
    }

    pub async fn addr(&self) -> SocketAddr {
        let node = self.inner.read().await;
        node.socket
    }
}

// Handle new peer connections
async fn node_server(node: Node, listener: TcpListener) {
    while let Ok((conn, addr)) = listener.accept().await {
        let mut peer = Peer {
            conn,
            addr,
            response: unbounded(),
            send_mutex: Arc::new(Mutex::new(())),
        };
        // The first message will always be the name.
        // TODO: Have a time-out here because other connections are blocked until the name arrives.
        if let Ok(Message::Register(name, new_addr)) = peer.receive().await {
            // Update address of peer to the one sent by it.
            peer.addr = new_addr;
            async_std::task::spawn(peer_task(node.clone(), peer.clone()));
            let mut node = node.inner.write().await;
            // TODO: Check if peer under this name exists or we already have this name and report error.
            // TODO: During the bootstrap process we will double connect to the bootstrap node,
            //       once to get all peers, and once we receive the bootstrap node as a peer
            node.peers.insert(name, peer);
        } else {
            todo!("Handle wrong first message");
        }
    }
}

// A task running in the background and responding to messages from a peer connection.
async fn peer_task(node: Node, mut peer: Peer) {
    trace!("{} listening to {}", node.addr().await, peer.addr);
    while let Ok(message) = peer.receive().await {
        trace!("receiving from {}: {:?}", peer.addr, message);
        match message {
            Message::Register(_, _) => unreachable!("Can't get a name message at this point"),
            Message::GetPeers => {
                let node = node.inner.read().await;
                let mut peers: Vec<(String, SocketAddr)> = node
                    .peers
                    .iter()
                    .map(|(name, peer)| (name.clone(), peer.addr))
                    .collect();
                // Add itself to the peer list.
                peers.push((node.name.clone(), node.socket));
                if peer.send(Message::Peers(peers)).await.is_err() {
                    // TODO: If message can't be sent declare node as dead
                    break;
                }
            }
            Message::Peers(_) => unreachable!("Peers are only received during bootstrap"),
            Message::CreateEnvironment(config) => {
                let env = Environment::new(config);
                if let Ok(env) = env {
                    let mut node = node.inner.write().await;
                    let id = node.resources.add(Resource::Environment(env));
                    // TODO: Deal with error
                    peer.send(Message::Resource(id)).await.unwrap();
                } else {
                    // TODO: Deal with error
                    peer.send(Message::Error(env.err().unwrap().to_string()))
                        .await
                        .unwrap();
                }
            }
            Message::CreateModule(env_id, data) => {
                let mut node = node.inner.write().await;
                match node.resources.get(env_id) {
                    Some(Resource::Environment(ref env)) => {
                        let module = env.create_module(data).await.unwrap();
                        let id = node.resources.add(Resource::Module(module));
                        peer.send(Message::Resource(id)).await.unwrap();
                    }
                    _ => {
                        peer.send(Message::Error("Resource is not an environment".to_string()))
                            .await
                            .unwrap();
                    }
                };
            }
            Message::Spawn(module_id, entry) => {
                let mut node = node.inner.write().await;
                match node.resources.get(module_id) {
                    Some(Resource::Module(ref module)) => {
                        let (_, process) = module.spawn(&entry, vec![], None).await.unwrap();
                        let id = node.resources.add(Resource::Process(Box::new(process)));
                        peer.send(Message::Resource(id)).await.unwrap();
                    }
                    _ => {
                        peer.send(Message::Error("Resource is not a module".to_string()))
                            .await
                            .unwrap();
                    }
                };
            }
            Message::Resource(id) => peer.set_response(Response::Resource(id)).await,
            Message::Error(error) => peer.set_response(Response::Error(error)).await,
        }
    }
}

#[derive(Clone)]
pub struct Peer {
    conn: TcpStream,
    addr: SocketAddr,
    response: (Sender<Response>, Receiver<Response>),
    send_mutex: Arc<Mutex<()>>,
}

impl Peer {
    async fn send(&mut self, msg: Message) -> Result<()> {
        // Multiple parts of the VM hold onto the peer and can send messages to it.
        // To avoid interleaved messages a mutex is used.
        // TODO: Should this be a queue instead?
        trace!("sending to {}: {:?}", self.addr, msg);
        let _lock = self.send_mutex.lock().await;

        let message = serialize(&msg)?;
        // Prefix message with size as little-endian u32 value.
        let size = (message.len() as u32).to_le_bytes();
        self.conn.write_all(&size).await?;
        self.conn.write_all(&message).await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Message> {
        let mut size = [0u8; 4];
        self.conn.read_exact(&mut size).await?;
        let size = u32::from_le_bytes(size);
        let mut buffer = vec![0u8; size as usize];
        self.conn.read_exact(&mut buffer).await?;
        Ok(deserialize(&buffer)?)
    }

    async fn set_response(&mut self, res: Response) {
        // The receiver side can't be closed at this time and it's safe to unwrap here.
        self.response.0.send(res).await.unwrap();
    }

    async fn response(&mut self) -> Response {
        // The sender side can't be closed at this time and it's safe to unwrap here.
        self.response.1.recv().await.unwrap()
    }

    pub async fn create_environment(&mut self, config: EnvConfig) -> Result<u64> {
        self.send(Message::CreateEnvironment(config)).await.unwrap();
        match self.response().await {
            Response::Resource(id) => Ok(id),
            Response::Error(error) => Err(anyhow!(error)),
        }
    }

    pub async fn create_module(&mut self, env_id: u64, data: Vec<u8>) -> Result<u64> {
        self.send(Message::CreateModule(env_id, data))
            .await
            .unwrap();
        match self.response().await {
            Response::Resource(id) => Ok(id),
            Response::Error(error) => Err(anyhow!(error)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    // Register yourself to another node
    Register(String, SocketAddr),
    // Request peers from another node
    GetPeers,
    // Receive peers from a node
    Peers(Vec<(String, SocketAddr)>),
    // Create environment on remote node.
    CreateEnvironment(EnvConfig),
    // Send module to remote node's environment.
    CreateModule(u64, Vec<u8>),
    // Spawn a process on a remote node.
    Spawn(u64, String),
    // Remote resource
    Resource(u64),
    // Error message
    Error(String),
}

enum Response {
    // Remote resource
    Resource(u64),
    // Error message
    Error(String),
}

enum Resource {
    Environment(Environment),
    Module(Module),
    Process(Box<dyn Process>),
}

#[cfg(test)]
mod tests {
    use crate::EnvConfig;

    use super::Node;

    #[async_std::test]
    async fn single_node_startup() {
        let node1 = Node::new("node1".into(), "localhost:35555", None)
            .await
            .unwrap();
        let peers1 = node1.peers().await;
        assert_eq!(peers1.len(), 0);
    }

    #[async_std::test]
    async fn dual_node_startup() {
        let node1 = Node::new("node1".into(), "localhost:35556", None)
            .await
            .unwrap();
        let node2 = Node::new("node2".into(), "localhost:35557", Some("localhost:35556"))
            .await
            .unwrap();
        // Let the nodes sync up before continuing
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        let peers1 = node1.peers().await;
        assert_eq!(peers1.len(), 1);
        let peer1_name = peers1.iter().next().unwrap().0;
        assert_eq!(peer1_name, "node2");

        let peers2 = node2.peers().await;
        assert_eq!(peers2.len(), 1);
        let peer2_name = peers2.iter().next().unwrap().0;
        assert_eq!(peer2_name, "node1");
    }

    #[async_std::test]
    async fn triple_node_setup() {
        let node1 = Node::new("node1".into(), "localhost:35558", None)
            .await
            .unwrap();
        let node2 = Node::new("node2".into(), "localhost:35559", Some("localhost:35558"))
            .await
            .unwrap();
        let node3 = Node::new("node3".into(), "localhost:35560", Some("localhost:35559"))
            .await
            .unwrap();
        // Let the nodes sync up before continuing
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        let peers1 = node1.peers().await;
        assert_eq!(peers1.len(), 2);
        let peers2 = node2.peers().await;
        assert_eq!(peers2.len(), 2);
        let peers3 = node3.peers().await;
        assert_eq!(peers3.len(), 2);
    }

    #[async_std::test]
    async fn create_remote_env() {
        // Capture log in test
        let _ = env_logger::builder().is_test(true).try_init();

        let node1 = Node::new("node1".into(), "localhost:35561", None)
            .await
            .unwrap();
        let node2 = Node::new("node2".into(), "localhost:35562", Some("localhost:35561"))
            .await
            .unwrap();
        // Let the nodes sync up before continuing
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        // Create environment on node2
        let mut peers1 = node1.peers().await;
        let peer2 = peers1.get_mut("node2").unwrap();
        let config = EnvConfig::default();
        let id = peer2.create_environment(config).await.unwrap();

        // Check if config exists on node2
        let node2 = node2.inner.read().await;
        let resource = node2.resources.get(id);
        assert!(resource.is_some());
    }
}
