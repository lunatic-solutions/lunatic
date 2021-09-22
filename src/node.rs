use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{anyhow, Result};
use async_std::{
    io::{ReadExt, WriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock},
};
use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};

use crate::{module::Module, state::HashMapId, Environment, Process};

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
    pub async fn new(
        name: String,
        socket: SocketAddr,
        bootstrap: Option<SocketAddr>,
    ) -> Result<Node> {
        // Bind itself to a socket
        let listener = TcpListener::bind(socket).await?;

        // Bootstrap all peers from one.
        let peers = if let Some(bootstrap) = bootstrap {
            let bootstrap_conn = TcpStream::connect(bootstrap).await?;
            let mut bootstrap_peer = Peer {
                conn: bootstrap_conn,
                addr: bootstrap,
                send_mutex: Arc::new(Mutex::new(())),
            };
            // Register ourself at the bootstrap peer
            bootstrap_peer
                .send(Message::Register(name.clone(), socket))
                .await?;
            // Ask the bootstrap for all other peers. The returned list will also contain the
            // bootstrap one.
            bootstrap_peer.send(Message::GetPeers).await?;
            if let Message::Peers(peer_infos) = bootstrap_peer.receive().await? {
                let mut peers = HashMap::new();
                for (name, addr) in peer_infos.into_iter() {
                    let peer_conn = TcpStream::connect(addr).await?;
                    let peer = Peer {
                        conn: peer_conn,
                        addr,
                        send_mutex: Arc::new(Mutex::new(())),
                    };
                    // Register ourself
                    bootstrap_peer
                        .send(Message::Register(name.clone(), socket))
                        .await?;
                    peers.insert(name, peer);
                }
                peers
            } else {
                return Err(anyhow!("Unexpected message from bootstrap node"));
            }
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

        async_std::task::spawn(node_server(node.clone(), listener));

        Ok(node)
    }
}

// Handle new peer connections
async fn node_server(node: Node, listener: TcpListener) {
    while let Ok((conn, addr)) = listener.accept().await {
        let mut peer = Peer {
            conn,
            addr,
            send_mutex: Arc::new(Mutex::new(())),
        };
        // The first message will always be the name.
        // TODO: Have a time-out here because other connections are blocked until the name arrives.
        if let Ok(Message::Register(name, new_addr)) = peer.receive().await {
            // Update address of peer to the one sent by it.
            peer.addr = new_addr;
            async_std::task::spawn(peer_task(node.clone(), peer.clone()));
            let mut node = node.inner.write().await;
            // TODO: Check if peer under this name exists and report error.
            node.peers.insert(name, peer);
        } else {
            todo!("Handle wrong first message");
        }
    }
}

// A task running in the background and responding to messages from a peer connection.
async fn peer_task(node: Node, mut peer: Peer) {
    while let Ok(message) = peer.receive().await {
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
                    // TODO: If message can't be send declare node as dead
                    break;
                }
            }
            Message::Peers(_) => unreachable!("Peers are only received during bootstrap"),
        }
    }
}

#[derive(Clone)]
struct Peer {
    conn: TcpStream,
    addr: SocketAddr,
    send_mutex: Arc<Mutex<()>>,
}

impl Peer {
    async fn send(&mut self, msg: Message) -> Result<()> {
        // Multiple parts of the VM hold onto the peer and can send messages to it.
        // To avoid interleaved messages a mutex is used.
        // TODO: Should this be a queue instead?
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
}

#[derive(Serialize, Deserialize)]
enum Message {
    // Register yourself to another node
    Register(String, SocketAddr),
    // Request peers from another node
    GetPeers,
    // Receive peers from a node
    Peers(Vec<(String, SocketAddr)>),
}

enum Resource {
    Environment(Environment),
    Module(Module),
    Process(Box<dyn Process>),
}
