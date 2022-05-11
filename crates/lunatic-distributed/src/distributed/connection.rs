use std::sync::Arc;

use anyhow::Result;
use async_std::{
    io::{ReadExt, WriteExt},
    net::TcpStream,
    sync::Mutex,
};
use bincode::{deserialize, serialize};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<TcpStream>,
    writer: Mutex<TcpStream>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            inner: Arc::new(InnerConnection {
                reader: Mutex::new(stream.clone()),
                writer: Mutex::new(stream),
            }),
        }
    }

    pub async fn send<T: Serialize>(&self, msg_id: u64, msg: T) -> Result<u64> {
        let message = serialize(&(msg_id, msg))?;
        // Prefix message with size as little-endian u32 value.
        let size = (message.len() as u32).to_le_bytes();
        let mut writer = self.inner.writer.lock().await;
        writer.write_all(&size).await?;
        writer.write_all(&message).await?;
        Ok(msg_id)
    }

    pub async fn receive<T: DeserializeOwned>(&self) -> Result<(u64, T)> {
        let mut reader = self.inner.reader.lock().await;
        let mut size = [0u8; 4];
        reader.read_exact(&mut size).await?;
        let size = u32::from_le_bytes(size);
        let mut buffer = vec![0u8; size as usize];
        reader.read_exact(&mut buffer).await?;
        Ok(deserialize(&buffer)?)
    }
}
