use super::{api, Message};

use smol::channel::{Receiver, RecvError};
use uptown_funk::{Executor, FromWasm, ToWasm};

#[derive(Clone)]
pub struct ChannelReceiver(pub Receiver<Message>);

impl ToWasm for ChannelReceiver {
    type To = u32;
    type State = api::ChannelState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        receiver: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.inner.borrow_mut().receivers.add(receiver))
    }
}

impl FromWasm for ChannelReceiver {
    type From = u32;
    type State = api::ChannelState;

    fn from(state: &mut Self::State, _: &impl Executor, id: u32) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.inner.borrow().receivers.get(id) {
            Some(receiver) => Ok(receiver.clone()),
            None => Err(uptown_funk::Trap::new("ChannelReceiver not found")),
        }
    }
}

impl ChannelReceiver {
    pub fn from(receiver: Receiver<Message>) -> Self {
        Self(receiver)
    }

    pub async fn receive(&self) -> Result<Message, RecvError> {
        self.0.recv().await
    }
}

pub enum ChannelReceiverResult {
    Ok(ChannelReceiver),
    Err(String),
}

impl ToWasm for ChannelReceiverResult {
    type To = u32;
    type State = api::ChannelState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            ChannelReceiverResult::Ok(receiver) => {
                Ok(state.inner.borrow_mut().receivers.add(receiver))
            }
            ChannelReceiverResult::Err(err) => Err(uptown_funk::Trap::new(err)),
        }
    }
}
