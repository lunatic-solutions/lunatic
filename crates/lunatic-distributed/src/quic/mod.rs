#[cfg(feature = "quic-quinn")]
mod quin;
#[cfg(feature = "quic-s2n")]
mod s2n;
#[cfg(feature = "tcp")]
mod tcp;

use std::{net::SocketAddr, time::Duration};

#[cfg(feature = "quic-quinn")]
pub use quin::*;

#[cfg(feature = "quic-s2n")]
pub use s2n::*;

#[cfg(feature = "tcp")]
pub use tcp::*;

pub async fn try_connect_forever(
    quic_client: &self::Client,
    addr: SocketAddr,
    name: &str,
) -> (self::SendStream, self::RecvStream) {
    loop {
        log::info!("Connecting to node {addr} - {name}");
        if let Ok(connection) = quic_client.connect(addr, name, 1).await {
            return connection;
        }
        log::warn!("Failed to connect to node {addr} - {name}, retrying...");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
