mod store;

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use lunatic::{
    abstract_process,
    ap::{Config, ProcessRef},
    ProcessName,
};
use lunatic_control::api::{NodeStart, Register};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::host::{self, CertPk};

use self::store::ControlServerStore;

#[derive(ProcessName)]
pub struct ControlServerProcess;

#[derive(Clone, Debug)]
pub struct ControlServer {
    ca_cert: CertPk,
    store: ControlServerStore,
    registrations: HashMap<u64, Registered>,
    nodes: HashMap<u64, NodeDetails>,
    modules: HashMap<u64, Vec<u8>>,
    next_registration_id: u64,
    next_node_id: u64,
    next_module_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registered {
    pub node_name: Uuid,
    pub csr_pem: String,
    pub cert_pem: String,
    pub auth_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeDetails {
    pub registration_id: u64,
    pub status: i16,
    pub created_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub node_address: String,
    pub attributes: BincodeJsonValue,
}

impl ControlServer {
    pub fn lookup() -> Option<ProcessRef<Self>> {
        ProcessRef::lookup(&ControlServerProcess)
    }
}

#[abstract_process(visibility = pub)]
impl ControlServer {
    #[init]
    fn init(_: Config<Self>, ca_cert: CertPk) -> Result<Self, String> {
        Self::init_new(ca_cert).map_err(|err| err.to_string())
    }

    fn init_new(ca_cert: CertPk) -> anyhow::Result<Self> {
        let store = ControlServerStore::connect("control_server.db")?;

        store.init()?;

        let registrations = store.load_registrations()?;
        let nodes = store.load_nodes()?;
        let modules = store.load_modules()?;

        let next_registration_id = registrations.keys().fold(1, |max, k| max.max(k + 1));
        let next_node_id = nodes.keys().fold(1, |max, k| max.max(k + 1));
        let next_module_id = modules.keys().fold(1, |max, k| max.max(k + 1));

        Ok(ControlServer {
            ca_cert,
            store,
            registrations,
            nodes,
            modules,
            next_registration_id,
            next_node_id,
            next_module_id,
        })
    }

    #[handle_message]
    pub fn register(&mut self, reg: Register, cert_pem: String, auth_token: String) {
        let id = self.next_registration_id;
        self.next_registration_id += 1;
        let registered = Registered {
            node_name: reg.node_name,
            csr_pem: reg.csr_pem,
            cert_pem,
            auth_token,
        };
        self.store.add_registration(id, &registered);
        self.registrations.insert(id, registered);
    }

    #[handle_request]
    pub fn start_node(&mut self, registration_id: u64, data: NodeStart) -> (u64, String) {
        let id = self.next_node_id;
        self.next_node_id += 1;
        let details = NodeDetails {
            registration_id,
            status: 0,
            created_at: Utc::now(),
            stopped_at: None,
            node_address: data.node_address.to_string(),
            attributes: serde_json::json!(data.attributes).into(),
        };
        self.store.add_node(id, &details);
        self.nodes.insert(id, details);
        (id, data.node_address.to_string())
    }

    #[handle_message]
    pub fn stop_node(&mut self, reg_id: u64) {
        if let Some(mut node) = self.nodes.get_mut(&reg_id) {
            node.status = 2;
            node.stopped_at = Some(Utc::now());
            self.store.add_node(reg_id, node);
        }
    }

    #[handle_request]
    pub fn add_module(&mut self, bytes: Vec<u8>) -> u64 {
        let id = self.next_module_id;
        self.next_module_id += 1;
        self.store.add_module(id, bytes.clone());
        self.modules.insert(id, bytes);
        id
    }

    #[handle_request]
    pub fn get_nodes(&self) -> HashMap<u64, NodeDetails> {
        self.nodes.clone()
    }

    #[handle_request]
    pub fn get_registrations(&self) -> HashMap<u64, Registered> {
        self.registrations.clone()
    }

    #[handle_request]
    pub fn get_modules(&self) -> HashMap<u64, Vec<u8>> {
        self.modules.clone()
    }

    #[handle_request]
    pub fn root_cert(&self) -> String {
        self.ca_cert.cert.clone()
    }

    #[handle_request]
    pub fn sign_node(&self, csr_pem: String) -> String {
        host::sign_node(&self.ca_cert.cert, &self.ca_cert.pk, &csr_pem)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BincodeJsonValue(pub serde_json::Value);

impl From<serde_json::Value> for BincodeJsonValue {
    fn from(value: serde_json::Value) -> Self {
        BincodeJsonValue(value)
    }
}

impl From<BincodeJsonValue> for serde_json::Value {
    fn from(value: BincodeJsonValue) -> Self {
        value.0
    }
}

impl Deref for BincodeJsonValue {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BincodeJsonValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Serialize for BincodeJsonValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = serde_json::to_vec(&self.0)
            .map_err(|err| <S::Error as serde::ser::Error>::custom(err.to_string()))?;

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for BincodeJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BincodeJsonValueVisitor;

        impl<'de> serde::de::Visitor<'de> for BincodeJsonValueVisitor {
            type Value = BincodeJsonValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a byte slice of json")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::from_slice(v)
                    .map(BincodeJsonValue)
                    .map_err(|err| E::custom(err.to_string()))
            }
        }

        deserializer.deserialize_bytes(BincodeJsonValueVisitor)
    }
}
