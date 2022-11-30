use anyhow::{Context, Result};
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
    reg: Registration,
    node_id: u64,
    http_client: HttpClient,
    next_message_id: AtomicU64,
    next_query_id: AtomicU64,
    node_queries: DashMap<u64, Vec<u64>>,
    nodes: DashMap<u64, NodeInfo>,
    node_ids: RwLock<Vec<u64>>,
}

impl Client {
    pub async fn new(
        http_client: HttpClient,
        reg: Registration,
        node_address: SocketAddr,
        attributes: HashMap<String, String>,
    ) -> Result<Self> {
        let node_id = Self::start(
            &http_client,
            &reg,
            NodeStart {
                node_address,
                attributes,
            },
        )
        .await?;

        let client = Client {
            inner: Arc::new(InnerClient {
                reg,
                node_id,
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
        http_client: &HttpClient,
        control_url: Url,
        node_name: uuid::Uuid,
        csr_pem: String,
    ) -> Result<Registration> {
        let reg = Register { node_name, csr_pem };
        Self::send_registration(http_client, control_url, reg).await
    }

    pub fn reg(&self) -> Registration {
        self.inner.reg.clone()
    }

    pub fn node_id(&self) -> u64 {
        self.inner.node_id
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
        client: &HttpClient,
        url: Url,
        reg: Register,
    ) -> Result<Registration> {
        let resp: Registration = client
            .post(url)
            .json(&reg)
            .send()
            .await
            .with_context(|| "Error sending HTTP registration request.")?
            .error_for_status()
            .with_context(|| "HTTP registration request returned an error response.")?
            .json()
            .await
            .with_context(|| "Error parsing the registration request JSON.")?;
        Ok(resp)
    }

    async fn start(client: &HttpClient, reg: &Registration, start: NodeStart) -> Result<u64> {
        let resp: NodeStarted = client
            .post(&reg.urls.node_started)
            .json(&start)
            .bearer_auth(&reg.authentication_token)
            .header(
                "x-lunatic-node-name",
                &reg.node_name.hyphenated().to_string(),
            )
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.node_id as u64)
    }

    pub async fn get<T: DeserializeOwned>(&self, url: &str, query: Option<&str>) -> Result<T> {
        let mut url: Url = url.parse()?;
        url.set_query(query);

        let resp: T = self
            .inner
            .http_client
            .get(url.clone())
            .bearer_auth(&self.inner.reg.authentication_token)
            .header(
                "x-lunatic-node-name",
                &self.inner.reg.node_name.hyphenated().to_string(),
            )
            .send()
            .await
            .with_context(|| format!("Error sending HTTP GET request: {}.", &url))?
            .error_for_status()
            .with_context(|| format!("HTTP GET request returned an error response: {}", &url))?
            .json()
            .await
            .with_context(|| format!("Error parsing the HTTP GET request JSON: {}", &url))?;

        Ok(resp)
    }

    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, url: &str, data: T) -> Result<R> {
        let url: Url = url.parse()?;

        let resp: R = self
            .inner
            .http_client
            .post(url.clone())
            .json(&data)
            .bearer_auth(&self.inner.reg.authentication_token)
            .header(
                "x-lunatic-node-name",
                &self.inner.reg.node_name.hyphenated().to_string(),
            )
            .send()
            .await
            .with_context(|| format!("Error sending HTTP POST request: {}.", &url))?
            .error_for_status()
            .with_context(|| format!("HTTP POST request returned an error response: {}", &url))?
            .json()
            .await
            .with_context(|| format!("Error parsing the HTTP POST request JSON: {}", &url))?;

        Ok(resp)
    }

    pub async fn refresh_nodes(&self) -> Result<()> {
        let resp: NodesList = self.get(&self.inner.reg.urls.nodes, None).await?;
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
        let resp: NodesList = self
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
        log::info!("Get module {module_id}");
        let url = self
            .inner
            .reg
            .urls
            .get_module
            .replace("{id}", &module_id.to_string());
        let resp: ModuleBytes = self.get(&url, None).await?;
        Ok(resp.bytes)
    }

    pub async fn add_module(&self, module: Vec<u8>) -> Result<RawWasm> {
        let url = &self.inner.reg.urls.add_module;
        let resp: ModuleId = self
            .post(
                &url,
                &AddModule {
                    bytes: module.clone(),
                },
            )
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
