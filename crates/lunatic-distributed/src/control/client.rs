use anyhow::Result;
use dashmap::DashMap;
use lunatic_process::runtimes::RawWasm;
use reqwest::{Client as HttpClient, Url};
use serde::{de::DeserializeOwned, Serialize};
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
    reg: RegisterResponse,
    http_client: HttpClient,
    next_message_id: AtomicU64,
    next_query_id: AtomicU64,
    node_queries: DashMap<u64, Vec<u64>>,
    nodes: DashMap<u64, NodeInfo>,
    node_ids: RwLock<Vec<u64>>,
}

impl Client {
    pub async fn from_registration(reg: RegisterResponse, http_client: HttpClient) -> Result<Self> {
        let client = Client {
            inner: Arc::new(InnerClient {
                reg,
                http_client,
                next_message_id: AtomicU64::new(1),
                node_queries: DashMap::new(),
                next_query_id: AtomicU64::new(1),
                nodes: Default::default(),
                node_ids: Default::default(),
            }),
        };

        tokio::task::spawn(refresh_nodes_task(client.clone()));
        client.refresh_nodes().await?;

        Ok(client)
    }

    pub async fn register(
        node_address: SocketAddr,
        node_name: uuid::Uuid,
        attributes: HashMap<String, String>,
        control_url: Url,
        http_client: HttpClient,
        csr_pem: String,
    ) -> Result<Self> {
        let reg = Register {
            node_address,
            node_name,
            attributes,
            csr_pem,
        };

        let reg = Self::send_registration(control_url, &http_client, reg).await?;
        Self::from_registration(reg, http_client).await
    }

    pub fn reg(&self) -> RegisterResponse {
        self.inner.reg.clone()
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

    async fn send_registration(
        url: Url,
        client: &HttpClient,
        reg: Register,
    ) -> Result<RegisterResponse> {
        let resp: RegisterResponse = client.post(url).json(&reg).send().await?.json().await?;
        Ok(resp)
    }

    pub async fn get<T: DeserializeOwned>(&self, url: &str, query: Option<&str>) -> Result<T> {
        let mut url: Url = url.parse()?;
        url.set_query(query);

        let resp: T = self
            .inner
            .http_client
            .get(url)
            .bearer_auth(&self.inner.reg.authentication_token)
            .header(
                "x-lunatic-node-name",
                &self.inner.reg.node_name.hyphenated().to_string(),
            )
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    // TODO handle HTTP codes and errors with a proper message/result
    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, url: &str, data: T) -> Result<R> {
        let url: Url = url.parse()?;
        let resp: R = self
            .inner
            .http_client
            .post(url)
            .json(&data)
            .bearer_auth(&self.inner.reg.authentication_token)
            .header(
                "x-lunatic-node-name",
                &self.inner.reg.node_name.hyphenated().to_string(),
            )
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn refresh_nodes(&self) -> Result<()> {
        let resp: NodesResponse = self.get(&self.inner.reg.urls.nodes, None).await?;
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

    pub async fn notify_node_stopped(&self) -> Result<()> {
        self.post(&self.inner.reg.urls.node_stopped, ()).await?;
        Ok(())
    }

    pub fn node_info(&self, node_id: u64) -> Option<NodeInfo> {
        self.inner.nodes.get(&node_id).map(|e| e.clone())
    }

    pub fn node_ids(&self) -> Vec<u64> {
        self.inner.node_ids.read().unwrap().clone()
    }

    pub async fn lookup_nodes(&self, query: &str) -> Result<(u64, usize)> {
        let resp: NodesResponse = self
            .get(&self.inner.reg.urls.get_nodes, Some(query))
            .await?;
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

    pub async fn get_module(&self, module_id: u64) -> Result<Vec<u8>> {
        let url: Url = self
            .inner
            .reg
            .urls
            .get_module
            .replace("{id}", &module_id.to_string())
            .parse()?;
        let resp: ModuleResponse = self.inner.http_client.get(url).send().await?.json().await?;
        Ok(resp.bytes)
    }

    pub async fn add_module(&self, module: Vec<u8>) -> Result<RawWasm> {
        let url: Url = self.inner.reg.urls.add_module.parse()?;
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
