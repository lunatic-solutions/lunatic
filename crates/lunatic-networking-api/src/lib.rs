mod dns;
mod tcp;
mod udp;

use std::convert::TryInto;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_error_api::ErrorCtx;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;

use tokio::net::{TcpListener, TcpStream, UdpSocket};
use wasmtime::{Caller, Linker};
use wasmtime::{Memory, Trap};

use lunatic_common_api::IntoTrap;

pub use dns::DnsIterator;

pub struct TcpConnection {
    pub reader: Mutex<OwnedReadHalf>,
    pub writer: Mutex<OwnedWriteHalf>,
    pub read_timeout: Mutex<Option<Duration>>,
    pub write_timeout: Mutex<Option<Duration>>,
    pub peek_timeout: Mutex<Option<Duration>>,
}

impl TcpConnection {
    pub fn new(stream: TcpStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        TcpConnection {
            reader: Mutex::new(read_half),
            writer: Mutex::new(write_half),
            read_timeout: Mutex::new(None),
            write_timeout: Mutex::new(None),
            peek_timeout: Mutex::new(None),
        }
    }
}

pub type TcpListenerResources = HashMapId<TcpListener>;
pub type TcpStreamResources = HashMapId<Arc<TcpConnection>>;
pub type UdpResources = HashMapId<Arc<UdpSocket>>;
pub type DnsResources = HashMapId<DnsIterator>;

pub trait NetworkingCtx {
    fn tcp_listener_resources(&self) -> &TcpListenerResources;
    fn tcp_listener_resources_mut(&mut self) -> &mut TcpListenerResources;
    fn tcp_stream_resources(&self) -> &TcpStreamResources;
    fn tcp_stream_resources_mut(&mut self) -> &mut TcpStreamResources;
    fn udp_resources(&self) -> &UdpResources;
    fn udp_resources_mut(&mut self) -> &mut UdpResources;
    fn dns_resources(&self) -> &DnsResources;
    fn dns_resources_mut(&mut self) -> &mut DnsResources;
}

// Register the networking APIs to the linker
pub fn register<T: NetworkingCtx + ErrorCtx + Send + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    dns::register(linker)?;
    tcp::register(linker)?;
    udp::register(linker)?;
    Ok(())
}

fn socket_address<T: NetworkingCtx>(
    caller: &Caller<T>,
    memory: &Memory,
    addr_type: u32,
    addr_u8_ptr: u32,
    port: u32,
    flow_info: u32,
    scope_id: u32,
) -> Result<SocketAddr, Trap> {
    Ok(match addr_type {
        4 => {
            let ip = memory
                .data(&caller)
                .get(addr_u8_ptr as usize..(addr_u8_ptr + 4) as usize)
                .or_trap("lunatic::network::socket_address*")?;
            let addr = <Ipv4Addr as From<[u8; 4]>>::from(ip.try_into().expect("exactly 4 bytes"));
            SocketAddrV4::new(addr, port as u16).into()
        }
        6 => {
            let ip = memory
                .data(&caller)
                .get(addr_u8_ptr as usize..(addr_u8_ptr + 16) as usize)
                .or_trap("lunatic::network::socket_address*")?;
            let addr = <Ipv6Addr as From<[u8; 16]>>::from(ip.try_into().expect("exactly 16 bytes"));
            SocketAddrV6::new(addr, port as u16, flow_info, scope_id).into()
        }
        _ => return Err(Trap::new("Unsupported address type in socket_address*")),
    })
}
