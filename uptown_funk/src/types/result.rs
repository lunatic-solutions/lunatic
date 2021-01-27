use crate::{ToWasm, Trap};

impl<T: ToWasm> ToWasm for Result<T, Trap> {
    type To = T::To;
    type State = T::State;

    fn to(
        state: &mut Self::State,
        executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        host_value.and_then(|v| ToWasm::to(state, executor, v))
    }
}

pub trait HasOk {
    fn ok() -> Self;
}


impl<T: ToWasm + HasOk> ToWasm for Result<(), T> {
    type To = T::To;
    type State = T::State;

    fn to(
        state: &mut Self::State,
        executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        match host_value {
            Ok(_) => T::to(state, executor, T::ok()),
            Err(e) => T::to(state, executor, e),
        }
    }
}