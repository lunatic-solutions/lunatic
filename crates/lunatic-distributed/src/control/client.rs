use anyhow::Result;
use dashmap::DashMap;
use lunatic_process::runtimes::RawWasm;
use reqwest::Client as HttpClient;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic, atomic::AtomicU64, Arc, RwLock},
    time::Duration,
};

use crate::{control::api::*, NodeInfo};

#[derive(Clone)]
pub struct Client {
    inner: Arc<InnerClient>,
}

pub struct InnerClient {
    http_client: HttpClient,
    next_message_id: AtomicU64,
    next_query_id: AtomicU64,
    node_addr: SocketAddr,
    node_name: String,
    control_url: String,
    node_queries: DashMap<u64, Vec<u64>>,
    nodes: DashMap<u64, NodeInfo>,
    node_ids: RwLock<Vec<u64>>,
    attributes: HashMap<String, String>,
}

impl Client {
    pub async fn register(
        node_addr: SocketAddr,
        node_name: String,
        attributes: HashMap<String, String>,
        control_url: String,
        http_client: HttpClient,
        signing_request: String,
    ) -> Result<(u64, Self, String)> {
        let client = Client {
            inner: Arc::new(InnerClient {
                http_client,
                next_message_id: AtomicU64::new(1),
                control_url,
                node_addr,
                node_name: node_name.clone(),
                node_queries: DashMap::new(),
                next_query_id: AtomicU64::new(1),
                nodes: Default::default(),
                node_ids: Default::default(),
                attributes,
            }),
        };
        let reg = client.send_registration(signing_request).await?;
        tokio::task::spawn(refresh_nodes_task(client.clone()));
        client.refresh_nodes().await?;

        Ok((reg.node_id as u64, client, reg.cert_pem))
    }

    pub fn next_message_id(&self) -> u64 {
        self.inner
            .next_message_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    pub fn next_query_id(&self) -> u64 {
        self.inner
            .next_query_id
            .fetch_add(1, atomic::Ordering::Relaxed)
    }

    async fn send_registration(&self, csr_pem: String) -> Result<RegisterResponse> {
        let reg = Register {
            node_address: self.inner.node_addr,
            node_name: self.inner.node_name.clone().parse().unwrap(), // TODO node name UUID?
            attributes: self.inner.attributes.clone(),
            csr_pem,
        };
        let url = format!("{}/api/control/register", self.inner.control_url);
        let resp: RegisterResponse = self
            .inner
            .http_client
            .post(url)
            .json(&reg)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn refresh_nodes(&self) -> Result<()> {
        let url = format!("{}/api/control/nodes", self.inner.control_url);
        let resp: NodesResponse = self.inner.http_client.get(url).send().await?.json().await?;
        let mut node_ids = vec![];
        for node in resp.nodes {
            let id = node.id;
            node_ids.push(id);
            if !self.inner.nodes.contains_key(&id) {
                self.inner.nodes.insert(id, node);
            }
        }
        if let Ok(mut self_node_ids) = self.inner.node_ids.write() {
            *self_node_ids = node_ids;
        }
        Ok(())
    }

    pub async fn deregister(&self, node_id: u64) {
        let url = format!(
            "{}/api/control/deregister/{}",
            self.inner.control_url, node_id
        );
        self.inner.http_client.post(url).send().await.ok();
    }

    pub fn node_info(&self, node_id: u64) -> Option<NodeInfo> {
        self.inner.nodes.get(&node_id).map(|e| e.clone())
    }

    pub fn node_ids(&self) -> Vec<u64> {
        self.inner.node_ids.read().unwrap().clone()
    }

    pub async fn lookup_nodes(&self, query: &str) -> Result<(u64, usize)> {
        let url = format!(
            "{}/api/control/lookup_nodes?query={}",
            self.inner.control_url, query
        );
        let resp: NodesResponse = self.inner.http_client.get(url).send().await?.json().await?;
        let nodes: Vec<u64> = resp.nodes.into_iter().map(move |v| v.id).collect();
        let nodes_count = nodes.len();
        let query_id = self.next_query_id();
        self.inner.node_queries.insert(query_id, nodes);
        Ok((query_id, nodes_count))
    }

    pub fn query_result(&self, query_id: &u64) -> Option<(u64, Vec<u64>)> {
        self.inner.node_queries.remove(query_id)
    }

    pub fn node_count(&self) -> usize {
        self.inner.node_ids.read().unwrap().len()
    }

    pub async fn get_module(&self, module_id: u64) -> Option<Vec<u8>> {
        let url = format!(
            "{}/api/control/module/{}",
            self.inner.control_url, module_id
        );
        if let Ok(resp) = self.inner.http_client.get(url).send().await {
            if let Ok(resp) = resp.json::<ModuleResponse>().await {
                return Some(resp.bytes);
            }
        }
        None
    }

    pub async fn add_module(&self, module: Vec<u8>) -> Result<RawWasm> {
        let url = format!("{}/api/control/module", self.inner.control_url);
        let resp: AddModuleResponse = self
            .inner
            .http_client
            .post(url)
            .json(&AddModule {
                bytes: module.clone(),
            })
            .send()
            .await?
            .json()
            .await?;
        Ok(RawWasm::new(Some(resp.module_id), module))
    }
}

async fn refresh_nodes_task(client: Client) -> Result<()> {
    loop {
        client.refresh_nodes().await.ok();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
