use crate::{ToWasm, Trap};
use std::marker::PhantomData;
pub struct Status<S> {
    status: u32,
    _state: PhantomData<S>,
}

impl<S> ToWasm for Result<Status<S>, Trap> {
    type To = u32;
    type State = S;

    fn to(
        _state: &mut Self::State,
        _instance: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        host_value.map(|v| v.status)
    }
}

impl<S> From<u32> for Status<S> {
    fn from(v: u32) -> Self {
        Self {
            status: v,
            _state: PhantomData::default(),
        }
    }
}
