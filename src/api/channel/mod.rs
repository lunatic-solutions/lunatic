//! Channels allow for sending data between processes.
//!
//! Two processes don't share any memory and the only way of communicating with each other is through
//! messages. All data sent from one process to another is first copied from the heap of the source
//! process into the `Message` and then from the buffer to the heap of the receiving process.

pub mod api;
pub mod host_resources;
mod message;
mod receiver;
mod sender;

pub use message::Message;
pub use receiver::{ChannelReceiver, ChannelReceiverResult};
pub use sender::{ChannelSender, ChannelSenderResult};
