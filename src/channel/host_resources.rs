use crate::networking::TcpListener;
use crate::networking::TcpStream;
use crate::process::Process;

use std::convert::TryFrom;

use super::{ChannelReceiver, ChannelSender};

pub enum Resource {
    Empty,
    Process(Process),
    ChannelSender(ChannelSender),
    ChannelReceiver(ChannelReceiver),
    TcpListener(TcpListener),
    TcpStream(TcpStream),
}

impl From<Process> for Resource {
    fn from(process: Process) -> Self {
        Resource::Process(process)
    }
}

impl TryFrom<Resource> for Process {
    type Error = ();

    fn try_from(resource: Resource) -> Result<Process, ()> {
        match resource {
            Resource::Process(process) => Ok(process),
            _ => Err(()),
        }
    }
}

impl From<ChannelSender> for Resource {
    fn from(sender: ChannelSender) -> Self {
        Resource::ChannelSender(sender)
    }
}

impl TryFrom<Resource> for ChannelSender {
    type Error = ();

    fn try_from(resource: Resource) -> Result<ChannelSender, ()> {
        match resource {
            Resource::ChannelSender(sender) => Ok(sender),
            _ => Err(()),
        }
    }
}

impl From<ChannelReceiver> for Resource {
    fn from(receiver: ChannelReceiver) -> Self {
        Resource::ChannelReceiver(receiver)
    }
}

impl TryFrom<Resource> for ChannelReceiver {
    type Error = ();

    fn try_from(resource: Resource) -> Result<ChannelReceiver, ()> {
        match resource {
            Resource::ChannelReceiver(receiver) => Ok(receiver),
            _ => Err(()),
        }
    }
}

impl From<TcpListener> for Resource {
    fn from(tcp_listener: TcpListener) -> Self {
        Resource::TcpListener(tcp_listener)
    }
}

impl TryFrom<Resource> for TcpListener {
    type Error = ();

    fn try_from(resource: Resource) -> Result<TcpListener, ()> {
        match resource {
            Resource::TcpListener(tcp_listener) => Ok(tcp_listener),
            _ => Err(()),
        }
    }
}

impl From<TcpStream> for Resource {
    fn from(tcp_stream: TcpStream) -> Self {
        Resource::TcpStream(tcp_stream)
    }
}

impl TryFrom<Resource> for TcpStream {
    type Error = ();

    fn try_from(resource: Resource) -> Result<TcpStream, ()> {
        match resource {
            Resource::TcpStream(tcp_stream) => Ok(tcp_stream),
            _ => Err(()),
        }
    }
}
