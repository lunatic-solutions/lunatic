use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;

use bincode::{deserialize, serialize};
use s2n_quic::{
    stream::{BidirectionalStream, ReceiveStream, SendStream},
    Client, Server,
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Mutex,
};

#[derive(Clone)]
pub struct Connection {
    inner: Arc<InnerConnection>,
}

pub struct InnerConnection {
    reader: Mutex<ReceiveStream>,
    writer: Mutex<SendStream>,
}

impl Connection {
    pub fn new(stream: BidirectionalStream) -> Self {
        let (read_half, write_half) = stream.split();
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
        println!("Message {}", message.len());
        let mut writer = self.inner.writer.lock().await;
        println!("Message got writer {}", message.len());
        writer.write_all(&size).await?;
        writer.write_all(&message).await?;
        println!("Message written {}", message.len());
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

pub fn new_quic_client(ca_cert: &str) -> Result<Client> {
    Client::builder()
        .with_tls(ca_cert)?
        .with_io("0.0.0.0:0")?
        .start()
        .map_err(|_| anyhow::anyhow!("Failed to start QUIC client."))
}

pub fn new_quic_server(addr: SocketAddr, cert: &str, key: &str) -> Result<Server> {
    Server::builder()
        .with_tls((cert, key))?
        .with_io(addr)?
        .start()
        .map_err(|_| anyhow::anyhow!("Failed to start QUIC server."))
}
