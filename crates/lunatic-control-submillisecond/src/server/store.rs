use std::collections::HashMap;

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use lunatic::sqlite::{Query, SqliteClient, SqliteError};

use super::{BincodeJsonValue, NodeDetails, Registered};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlServerStore {
    client: SqliteClient,
}

impl ControlServerStore {
    pub fn connect(path: &str) -> Result<Self, SqliteError> {
        let client = SqliteClient::connect(path)?;
        Ok(ControlServerStore { client })
    }

    pub fn init(&self) -> anyhow::Result<()> {
        self.client.execute(
            r#"CREATE TABLE IF NOT EXISTS registrations (
                id INT PRIMARY KEY,
                node_name TEXT NOT NULL,
                csr_pem TEXT NOT NULL,
                cert_pem TEXT NOT NULL,
                auth_token TEXT NOT NULL
            )"#,
        )?;
        self.client.execute(
            r#"CREATE TABLE IF NOT EXISTS nodes (
                id INT PRIMARY KEY,
                registration_id INT NOT NULL,
                status INT NOT NULL,
                created_at DATETIME NOT NULL,
                stopped_at DATETIME,
                node_address TEXT NOT NULL,
                attributes BLOB
            )"#,
        )?;
        self.client.execute(
            "CREATE TABLE IF NOT EXISTS modules (id INT PRIMARY KEY, module BLOB NOT NULL)",
        )?;

        Ok(())
    }

    pub fn load_registrations(&self) -> anyhow::Result<HashMap<u64, Registered>> {
        self.client
            .prepare_query("SELECT id, node_name, csr_pem, cert_pem, auth_token FROM registrations")
            .execute_iter()
            .map(|columns| {
                let mut cols = columns.into_iter();
                Ok((
                    cols.next()
                        .and_then(|id| id.into_int64().map(|id| id as u64))
                        .ok_or_else(|| anyhow!("missing or invalid id"))?,
                    Registered {
                        node_name: cols
                            .next()
                            .and_then(|node_name| node_name.into_text())
                            .and_then(|node_name| node_name.parse().ok())
                            .ok_or_else(|| anyhow!("missing or invalid node_name"))?,
                        csr_pem: cols
                            .next()
                            .and_then(|csr_pem| csr_pem.into_text())
                            .ok_or_else(|| anyhow!("missing or invalid csr_pem"))?,
                        cert_pem: cols
                            .next()
                            .and_then(|cert_pem| cert_pem.into_text())
                            .ok_or_else(|| anyhow!("missing or invalid cert_pem"))?,
                        auth_token: cols
                            .next()
                            .and_then(|auth_token| auth_token.into_text())
                            .ok_or_else(|| anyhow!("missing or invalid auth_token"))?,
                    },
                ))
            })
            .collect()
    }

    pub fn load_nodes(&self) -> anyhow::Result<HashMap<u64, NodeDetails>> {
        self.client
            .prepare_query("SELECT id, registration_id, status, created_at, stopped_at, node_address, attributes FROM nodes")
            .execute_iter()
            .map(|columns| {
                let mut cols = columns.into_iter();
                Ok((
                    cols.next()
                        .and_then(|id| id.into_int64().map(|id| id as u64))
                        .ok_or_else(|| anyhow!("missing or invalid id"))?,
                    NodeDetails {
                        registration_id: cols
                            .next()
                            .and_then(|registration_id| registration_id.as_int_any())
                            .and_then(|registration_id| registration_id.try_into().ok())
                            .ok_or_else(|| anyhow!("missing or invalid registration_id"))?,
                        status: cols
                            .next()
                            .and_then(|status| status.as_int_any())
                            .and_then(|status| status.try_into().ok())
                            .ok_or_else(|| anyhow!("missing or invalid status"))?,
                        created_at: cols
                            .next()
                            .and_then(|created_at| created_at.into_text())
                            .and_then(|created_at| DateTime::parse_from_rfc3339(&created_at).ok())
                            .map(|created_at| created_at.with_timezone(&Utc))
                            .ok_or_else(|| anyhow!("missing or invalid created_at"))?,
                        stopped_at: cols
                            .next()
                            .and_then(|stopped_at| stopped_at.as_text().map(|stopped_at| DateTime::parse_from_rfc3339(&stopped_at).ok()).or_else(|| stopped_at.into_null().map(|_| None)))
                            .map(|created_at| created_at.map(|dt| dt.with_timezone(&Utc)))
                            .ok_or_else(|| anyhow!("missing or invalid stopped_at"))?,
                        node_address: cols
                            .next()
                            .and_then(|node_address| node_address.into_text())
                            .ok_or_else(|| anyhow!("missing or invalid node_address"))?,
                        attributes: cols
                            .next()
                            .and_then(|attributes| attributes.into_blob())
                            .and_then(|attributes| serde_json::from_slice(&attributes).ok().map(BincodeJsonValue))
                            .ok_or_else(|| anyhow!("missing or invalid attributes"))?,
                    },
                ))
            })
            .collect()
    }

    pub fn load_modules(&self) -> anyhow::Result<HashMap<u64, Vec<u8>>> {
        self.client
            .prepare_query("SELECT id, module FROM modules")
            .execute_iter()
            .map(|columns| {
                let mut cols = columns.into_iter();
                Ok((
                    cols.next()
                        .and_then(|id| id.into_int64().map(|id| id as u64))
                        .ok_or_else(|| anyhow!("missing or invalid id"))?,
                    cols.next()
                        .and_then(|registration_id| registration_id.into_blob())
                        .ok_or_else(|| anyhow!("missing or invalid module"))?,
                ))
            })
            .collect()
    }

    pub fn add_registration(&self, id: u64, registered: &Registered) {
        self.client
            .prepare_query(
                r#"
                INSERT INTO registrations (
                    id, node_name, csr_pem, cert_pem, auth_token
                ) VALUES (?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET
                    node_name=excluded.node_name,
                    csr_pem=excluded.csr_pem,
                    cert_pem=excluded.cert_pem,
                    auth_token=excluded.auth_token
                "#,
            )
            .bind(id as i64)
            .bind(registered.node_name.to_string())
            .bind(&registered.csr_pem)
            .bind(&registered.cert_pem)
            .bind(&registered.auth_token)
            .execute();
    }

    pub fn add_node(&self, id: u64, node: &NodeDetails) {
        self.client
            .prepare_query(
                r#"
                INSERT INTO nodes (
                    id,
                    registration_id,
                    status,
                    created_at,
                    stopped_at,
                    node_address,
                    attributes
                ) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET
                    registration_id=excluded.registration_id,
                    status=excluded.status,
                    created_at=excluded.created_at,
                    stopped_at=excluded.stopped_at,
                    node_address=excluded.node_address,
                    attributes=excluded.attributes
                "#,
            )
            .bind(id as i64)
            .bind(node.registration_id as i64)
            .bind(node.status as i32)
            .bind(node.created_at.to_rfc3339())
            .bind(node.stopped_at.map(|dt| dt.to_rfc3339()))
            .bind(&node.node_address)
            .bind(serde_json::to_vec(&node.attributes).unwrap())
            .execute();
    }

    pub fn add_module(&self, id: u64, module: Vec<u8>) {
        self.client
            .prepare_query(
                r#"
                INSERT INTO modules (id, module)
                VALUES (?, ?)
                ON CONFLICT(id) DO UPDATE SET
                module=excluded.module
                "#,
            )
            .bind(id as i64)
            .bind(module)
            .execute();
    }
}
