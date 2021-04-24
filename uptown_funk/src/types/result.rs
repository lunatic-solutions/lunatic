use crate::{ToWasm, Trap};

impl<S, T: ToWasm<S>> ToWasm<S> for Result<T, Trap> {
    type To = T::To;

    fn to(state: S, executor: &impl crate::Executor, host_value: Self) -> Result<Self::To, Trap> {
        host_value.and_then(|v| ToWasm::to(state, executor, v))
    }
}

pub trait HasOk {
    fn ok() -> Self;
}

impl<S, T: ToWasm<S> + HasOk> ToWasm<S> for Result<(), T> {
    type To = T::To;

    fn to(state: S, executor: &impl crate::Executor, host_value: Self) -> Result<Self::To, Trap> {
        match host_value {
            Ok(_) => T::to(state, executor, T::ok()),
            Err(e) => T::to(state, executor, e),
        }
    }
}
