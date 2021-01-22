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
