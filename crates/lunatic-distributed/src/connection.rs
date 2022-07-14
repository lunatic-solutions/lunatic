use std::sync::Arc;

use anyhow::Result;

use bincode::{deserialize, serialize};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::Mutex,
};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<OwnedReadHalf>,
    writer: Mutex<OwnedWriteHalf>,
}

// Connection implements length-prefix framing and bincode serialization/deserialization of messages
impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        Connection {
            inner: Arc::new(InnerConnection {
                reader: Mutex::new(read_half),
                writer: Mutex::new(write_half),
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
