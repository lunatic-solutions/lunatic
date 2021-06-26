use super::{api::ChannelState, host_resources::Resource, Message};

use smol::channel::{SendError, Sender};
use uptown_funk::{Executor, FromWasm, ToWasm};

#[derive(Clone)]
pub struct ChannelSender(pub Sender<Message>);

impl ToWasm<&mut ChannelState> for ChannelSender {
    type To = u32;

    fn to(
        state: &mut ChannelState,
        _: &impl Executor,
        sender: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.inner.borrow_mut().senders.add(sender))
    }
}

impl FromWasm<&mut ChannelState> for ChannelSender {
    type From = u32;

    fn from(state: &mut ChannelState, _: &impl Executor, id: u32) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.inner.borrow().senders.get(id) {
            Some(sender) => Ok(sender.clone()),
            None => Err(uptown_funk::Trap::new("ChannelSender not found")),
        }
    }
}

impl ChannelSender {
    pub async fn send(
        &self,
        slice: &[u8],
        host_resources: Vec<Resource>,
    ) -> Result<(), SendError<Message>> {
        let buffer = unsafe { Message::new(slice.as_ptr(), slice.len(), host_resources) };
        self.0.send(buffer).await
    }
}

pub enum ChannelSenderResult {
    Ok(ChannelSender),
    Err(String),
}

impl ToWasm<&mut ChannelState> for ChannelSenderResult {
    type To = u32;

    fn to(
        state: &mut ChannelState,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            ChannelSenderResult::Ok(sender) => Ok(state.inner.borrow_mut().senders.add(sender)),
            ChannelSenderResult::Err(err) => Err(uptown_funk::Trap::new(err)),
        }
    }
}
