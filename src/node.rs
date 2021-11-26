use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Result};
use async_map::AsyncMap;
use async_std::{
    channel::{unbounded, Sender},
    io::{ReadExt, WriteExt},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{Mutex, RwLock},
};
use bincode::{deserialize, serialize};
use log::trace;
use serde::{Deserialize, Serialize};
use wasmtime::Val;

use crate::{async_map, module::Module, state::HashMapId, EnvConfig, Environment, Process, Signal};

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
            let mut bootstrap_peer = Peer::new(bootstrap_conn, bootstrap);
            // Register ourself at the bootstrap peer
            bootstrap_peer
                .send(Message::Register(name.clone(), socket))
                .await?;
            // Ask the bootstrap for all other peers. The returned list will also contain the
            // bootstrap one.
            bootstrap_peer.send(Message::GetPeers).await?;
            if let Message::Peers(peer_infos) = bootstrap_peer.receive().await?.into() {
                let mut peers = HashMap::new();
                for (peer_name, peer_addr) in peer_infos.into_iter() {
                    // At this point we are also a peer of the bootstrap node and we want to skip
                    // over ourself.
                    if socket == peer_addr {
                        continue;
                    }

                    let peer_conn = TcpStream::connect(peer_addr).await?;
                    let mut peer = Peer::new(peer_conn, peer_addr);
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
        let mut peer = Peer::new(conn.clone(), addr);
        // The first message will always be the name.
        // TODO: Have a time-out here because other connections are blocked until the name arrives.
        if let Ok(msg) = peer.receive().await {
            if let Message::Register(name, new_addr) = msg.into() {
                // Use address provided by the peer when sending it to other nodes
                let peer = Peer::new(conn, new_addr);
                async_std::task::spawn(peer_task(node.clone(), peer.clone()));
                let mut node = node.inner.write().await;
                // TODO: Check if peer under this name exists or we already have this name and report error.
                // TODO: During the bootstrap process we will double connect to the bootstrap node,
                //       once to get all peers, and once we receive the bootstrap node as a peer
                node.peers.insert(name, peer);
            } else {
                todo!("Handle wrong first message");
            }
        } else {
            todo!("Handle error on receive");
        }
    }
}

// A task running in the background and responding to messages from a peer connection.
async fn peer_task(node: Node, mut peer: Peer) {
    trace!("{} listening to {}", node.addr().await, peer.addr());
    while let Ok(message) = peer.receive().await {
        trace!("receiving from {}: {:?}", peer.addr(), message);
        let tag = message.tag;
        let message = message.data;
        match message {
            Message::Register(_, _) => unreachable!("Can't get a name message at this point"),
            Message::GetPeers => {
                let node = node.inner.read().await;
                let mut peers: Vec<(String, SocketAddr)> = node
                    .peers
                    .iter()
                    .map(|(name, peer)| (name.clone(), peer.addr()))
                    .collect();
                // Add itself to the peer list.
                peers.push((node.name.clone(), node.socket));
                let tagged_msg = Message::Peers(peers).add_tag(tag);
                if peer.send(tagged_msg).await.is_err() {
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
                    let tagged_msg = Message::Resource(id).add_tag(tag);
                    peer.send(tagged_msg).await.unwrap();
                } else {
                    // TODO: Deal with error
                    let tagged_msg = Message::Error(env.err().unwrap().to_string()).add_tag(tag);
                    peer.send(tagged_msg).await.unwrap();
                }
            }
            Message::CreateModule(env_id, data) => {
                let mut node = node.inner.write().await;
                match node.resources.get(env_id) {
                    Some(Resource::Environment(ref env)) => {
                        let module = env.create_module(data).await.unwrap();
                        let id = node.resources.add(Resource::Module(module));
                        let tagged_msg = Message::Resource(id).add_tag(tag);
                        peer.send(tagged_msg).await.unwrap();
                    }
                    _ => {
                        let tagged_msg =
                            Message::Error("Resource is not an environment".to_string())
                                .add_tag(tag);
                        peer.send(tagged_msg).await.unwrap();
                    }
                };
            }
            Message::Spawn(module_id, entry, params, link) => {
                // Create local link to forward info
                let link: Option<(Option<i64>, Arc<dyn Process>)> = if let Some(link) = link {
                    // Spawn local proxy process that will forward link breakage information to remote node.
                    let proxy_process =
                        ProxyProcess::new(link.process_resource_id, peer.clone(), node.clone());
                    Some((link.tag, Arc::new(proxy_process)))
                } else {
                    None
                };
                let mut node = node.inner.write().await;
                match node.resources.get(module_id) {
                    Some(Resource::Module(ref module)) => {
                        let params = params.into_iter().map(Val::I32).collect();
                        let (_, process) = module.spawn(&entry, params, link).await.unwrap();
                        let id = node.resources.add(Resource::Process(Arc::new(process)));
                        let tagged_msg = Message::Resource(id).add_tag(tag);
                        peer.send(tagged_msg).await.unwrap();
                    }
                    _ => {
                        let tagged_msg =
                            Message::Error("Resource is not a module".to_string()).add_tag(tag);
                        peer.send(tagged_msg).await.unwrap();
                    }
                };
            }
            Message::Send(process_id, signal) => {
                let process = {
                    let node = node.inner.write().await;
                    if let Some(Resource::Process(process)) = node.resources.get(process_id) {
                        process.clone()
                    } else {
                        // TODO: Handle errror
                        unreachable!("Resources are never dropped")
                    }
                };
                process.send(signal.into(peer.clone(), node.clone()).await.unwrap());
            }
            Message::Resource(id) => peer.add_response(tag, Response::Resource(id)),
            Message::Error(error) => peer.add_response(tag, Response::Error(error)),
        }
    }
}

#[derive(Clone)]
pub struct Peer {
    inner: Arc<InnerPeer>,
}

pub struct InnerPeer {
    reader: Mutex<TcpStream>,
    writer: Mutex<TcpStream>,
    addr: SocketAddr,
    request_id: AtomicU64,
    response: AsyncMap<u64, Response>,
}

impl Peer {
    fn new(conn: TcpStream, addr: SocketAddr) -> Self {
        Peer {
            inner: Arc::new(InnerPeer {
                reader: Mutex::new(conn.clone()),
                writer: Mutex::new(conn),
                addr,
                request_id: AtomicU64::new(0),
                response: AsyncMap::default(),
            }),
        }
    }

    async fn send<M: Into<TaggedMessage>>(&mut self, msg: M) -> Result<()> {
        let msg: TaggedMessage = msg.into();
        trace!("sending to {}: {:?}", self.inner.addr, msg);
        let message = serialize(&msg)?;
        // Prefix message with size as little-endian u32 value.
        let size = (message.len() as u32).to_le_bytes();
        let mut writer = self.inner.writer.lock().await;
        writer.write_all(&size).await?;
        writer.write_all(&message).await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<TaggedMessage> {
        let mut reader = self.inner.reader.lock().await;
        let mut size = [0u8; 4];
        reader.read_exact(&mut size).await?;
        let size = u32::from_le_bytes(size);
        let mut buffer = vec![0u8; size as usize];
        reader.read_exact(&mut buffer).await?;
        Ok(deserialize(&buffer)?)
    }

    fn addr(&self) -> SocketAddr {
        self.inner.addr
    }

    async fn request(&mut self, msg: Message) -> Result<Response> {
        let tag = self.inner.request_id.fetch_add(1, Ordering::SeqCst);
        let msg = msg.add_tag(tag);
        self.send(msg).await?;
        Ok(self.inner.response.wait_remove(tag).await)
    }

    fn add_response(&mut self, tag: u64, response: Response) {
        self.inner.response.insert(tag, response);
    }

    pub async fn create_environment(&mut self, config: EnvConfig) -> Result<u64> {
        let response = self.request(Message::CreateEnvironment(config)).await?;
        match response {
            Response::Resource(id) => Ok(id),
            Response::Error(error) => Err(anyhow!(error)),
        }
    }

    pub async fn create_module(&mut self, env_id: u64, data: Vec<u8>) -> Result<u64> {
        let response = self.request(Message::CreateModule(env_id, data)).await?;
        match response {
            Response::Resource(id) => Ok(id),
            Response::Error(error) => Err(anyhow!(error)),
        }
    }

    // TODO: Support other params types than i32
    pub async fn spawn(
        &mut self,
        mod_id: u64,
        entry: String,
        params: Vec<i32>,
        link: Option<Link>,
    ) -> Result<u64> {
        let response = self
            .request(Message::Spawn(mod_id, entry, params, link))
            .await?;
        match response {
            Response::Resource(id) => Ok(id),
            Response::Error(error) => Err(anyhow!(error)),
        }
    }

    pub async fn send_signal(&mut self, proc_id: u64, signal: SignalOverNetwork) -> Result<()> {
        self.send(Message::Send(proc_id, signal)).await
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct TaggedMessage {
    tag: u64,
    data: Message,
}

impl From<Message> for TaggedMessage {
    fn from(message: Message) -> Self {
        TaggedMessage {
            tag: 0,
            data: message,
        }
    }
}

impl From<TaggedMessage> for Message {
    fn from(tagged_msg: TaggedMessage) -> Self {
        tagged_msg.data
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
    Spawn(u64, String, Vec<i32>, Option<Link>),
    // Send
    Send(u64, SignalOverNetwork),
    // Remote resource
    Resource(u64),
    // Error message
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SignalOverNetwork {
    DataMessage(DataMessageOverNetwork),
    SignalMessage(Option<i64>),
    Kill,
    DieWhenLinkDies(bool),
    Link(Option<i64>, u64),
    LinkDied(Option<i64>),
}

impl SignalOverNetwork {
    async fn from(signal: Signal, node: Node) -> Result<Self> {
        match signal {
            Signal::Message(message) => match message {
                crate::message::Message::Data(message) => {
                    let mut resources = Vec::with_capacity(message.resources.len());
                    for resource in message.resources.into_iter() {
                        match resource {
                            crate::message::Resource::None => {
                                return Err(anyhow!("Resource None can't be sent to another node"))
                            }
                            crate::message::Resource::Process(process) => {
                                let mut node = node.inner.write().await;
                                let id = node.resources.add(Resource::Process(process));
                                resources.push(id);
                            }
                            crate::message::Resource::TcpStream(_) => {
                                return Err(anyhow!(
                                    "Resource TcpStream can't be sent to another node"
                                ))
                            }
                        }
                    }

                    let msg = DataMessageOverNetwork {
                        buffer: message.buffer,
                        read_ptr: message.read_ptr,
                        tag: message.tag,
                        resources,
                    };
                    Ok(SignalOverNetwork::DataMessage(msg))
                }
                crate::message::Message::Signal(tag) => Ok(SignalOverNetwork::SignalMessage(tag)),
            },
            Signal::Kill => Ok(SignalOverNetwork::Kill),
            Signal::DieWhenLinkDies(flag) => Ok(SignalOverNetwork::DieWhenLinkDies(flag)),
            Signal::Link(tag, proc) => {
                let mut node = node.inner.write().await;
                let id = node.resources.add(Resource::Process(proc));
                Ok(SignalOverNetwork::Link(tag, id))
            }
            // TODO: Link & unlink may not work as the ID is lost through the proxy?
            Signal::UnLink(_) => todo!(),
            Signal::LinkDied(tag) => Ok(SignalOverNetwork::LinkDied(tag)),
        }
    }

    async fn into(self, peer: Peer, node: Node) -> Result<Signal> {
        match self {
            SignalOverNetwork::DataMessage(message) => {
                let mut resources = Vec::with_capacity(message.resources.len());
                for proc_id in message.resources.into_iter() {
                    // Remote resources can only be processes for now. Spawn local proxy processes.
                    let proxy_process = ProxyProcess::new(proc_id, peer.clone(), node.clone());
                    resources.push(crate::message::Resource::Process(Arc::new(proxy_process)));
                }
                let msg = crate::message::DataMessage {
                    buffer: message.buffer,
                    read_ptr: message.read_ptr,
                    tag: message.tag,
                    resources,
                };
                Ok(Signal::Message(crate::message::Message::Data(msg)))
            }
            SignalOverNetwork::SignalMessage(tag) => {
                Ok(Signal::Message(crate::message::Message::Signal(tag)))
            }
            SignalOverNetwork::Kill => Ok(Signal::Kill),
            SignalOverNetwork::DieWhenLinkDies(flag) => Ok(Signal::DieWhenLinkDies(flag)),
            SignalOverNetwork::Link(tag, id) => {
                let proxy_process = ProxyProcess::new(id, peer, node);
                Ok(Signal::Link(tag, Arc::new(proxy_process)))
            }
            SignalOverNetwork::LinkDied(tag) => Ok(Signal::LinkDied(tag)),
        }
    }
}

struct ProxyProcess {
    signal_mailbox: Sender<Signal>,
}

impl ProxyProcess {
    fn new(receiver_id: u64, mut peer: Peer, node: Node) -> ProxyProcess {
        let (signal_mailbox, receiver) = unbounded::<Signal>();
        let _ = async_std::task::spawn(async move {
            // TODO: Sync when remote process is dropped and propagate info to clean up resources.
            loop {
                let signal = receiver.recv().await;
                if let Ok(signal) = signal {
                    let sendable_signal = SignalOverNetwork::from(signal, node.clone()).await;
                    if let Ok(sendable_signal) = sendable_signal {
                        let result = peer.send_signal(receiver_id, sendable_signal).await;
                        if result.is_err() {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                };
            }
        });
        ProxyProcess { signal_mailbox }
    }
}

impl Process for ProxyProcess {
    fn id(&self) -> uuid::Uuid {
        uuid::Uuid::nil()
    }

    fn send(&self, signal: Signal) {
        let _ = self.signal_mailbox.try_send(signal);
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataMessageOverNetwork {
    buffer: Vec<u8>,
    read_ptr: usize,
    tag: Option<i64>,
    resources: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Link {
    tag: Option<i64>,
    process_resource_id: u64,
}

impl Message {
    fn add_tag(self, tag: u64) -> TaggedMessage {
        TaggedMessage { tag, data: self }
    }
}

#[derive(Debug)]
enum Response {
    // Remote resource
    Resource(u64),
    // Error message
    Error(String),
}

enum Resource {
    Environment(Environment),
    Module(Module),
    Process(Arc<dyn Process>),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{node::Resource, EnvConfig, Process, Signal};

    use super::{Link, Node};

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

    #[async_std::test]
    async fn spawn_remote_process() {
        let node1 = Node::new("node1".into(), "localhost:35563", None)
            .await
            .unwrap();
        let node2 = Node::new("node2".into(), "localhost:35564", Some("localhost:35563"))
            .await
            .unwrap();
        // Let the nodes sync up before continuing
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        // Create environment on node2
        let mut peers1 = node1.peers().await;
        let peer2 = peers1.get_mut("node2").unwrap();
        let config = EnvConfig::default();
        let env_id = peer2.create_environment(config).await.unwrap();
        let raw_module = std::fs::read("./target/wasm/hello.wasm").unwrap();
        let mod_id = peer2.create_module(env_id, raw_module).await.unwrap();
        let proc = peer2
            .spawn(mod_id, "hello".to_string(), vec![], None)
            .await
            .unwrap();

        // Check if config exists on node2
        let node2 = node2.inner.read().await;
        let resource = node2.resources.get(proc);
        assert!(resource.is_some());
    }

    // This test may hang if there is a race condition while linking over the network.
    #[async_std::test]
    async fn spawn_linked_remote_process() {
        // Capture log in test
        let _ = env_logger::builder().is_test(true).try_init();

        let node1 = Node::new("node1".into(), "localhost:35565", None)
            .await
            .unwrap();
        let _node2 = Node::new("node2".into(), "localhost:35566", Some("localhost:35565"))
            .await
            .unwrap();
        // Let the nodes sync up before continuing
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        // Create environment on node2
        let mut peers1 = node1.peers().await;
        let peer2 = peers1.get_mut("node2").unwrap();
        let config = EnvConfig::default();
        let env_id = peer2.create_environment(config).await.unwrap();
        let wasm_wat = r#"(module (func (export "hello") unreachable))"#;
        let wasm = wat::parse_str(wasm_wat).unwrap();
        let mod_id = peer2.create_module(env_id, wasm).await.unwrap();

        // Create native process to link it with remote one
        let (handle, process) = crate::spawn(|this, mailbox| async move {
            // Don't die if one of the link dies.
            this.send(Signal::DieWhenLinkDies(false));
            // Wait on link death
            match mailbox.pop(None).await {
                crate::message::Message::Data(_) => {
                    unreachable!("Only a signal can be received")
                }
                crate::message::Message::Signal(tag) => {
                    assert_eq!(tag, Some(1337));
                }
            }
            Ok(())
        });

        // Put the native process inside the local resources table of node1
        let process: Arc<dyn Process> = Arc::new(process);
        let id = {
            let mut node1 = node1.inner.write().await;
            node1.resources.add(Resource::Process(process))
        };

        // Spawn remote process and link them
        let _proc = peer2
            .spawn(
                mod_id,
                "hello".to_string(),
                vec![],
                Some(Link {
                    tag: Some(1337),
                    process_resource_id: id,
                }),
            )
            .await
            .unwrap();

        // Wait on native process to finish, indicating it received the correct signal
        handle.await;
    }
}
