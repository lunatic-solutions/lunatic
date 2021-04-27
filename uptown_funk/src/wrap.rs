use std::{rc::Rc, sync::{Arc, Mutex}};

use crate::{Executor, FromWasm, HostFunctions, ToWasm};
use wasmtime::Caller;

#[derive(Debug, Default)]
pub struct State {
    counter: u32,
    log: Vec<u32>,
}

struct CustomType(u32);

impl FromWasm<&Arc<Mutex<State>>> for CustomType {
    type From = u32;

    fn from(state: &Arc<Mutex<State>>, _: &impl Executor, from: u32) -> Result<Self, crate::Trap>
    where
        Self: Sized {
            Ok(CustomType(from + state.lock().unwrap().log.len() as u32))
    }
}

type CustomReturnType = ();

impl State {
    fn count(&mut self, val: CustomType) -> CustomReturnType {
        self.counter += val.0;
        self.log.push(val.0);
    }

    async fn count_async(&mut self, val: CustomType) -> CustomReturnType {
        self.counter += val.0;
        self.log.push(val.0);
    }
}

impl HostFunctions for State {
    type Wrap = Arc<Mutex<Self>>;
    type Return = Arc<Mutex<Self>>;

    fn split(self) -> (Self::Return, Self::Wrap) {
        let s =Arc::new(Mutex::new(self));
        (s.clone(), s)
    }

    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(api: Self::Wrap, executor: E, linker: &mut wasmtime::Linker)
    where
        E: crate::Executor + 'static,
    {
        let executor = Rc::new(executor);
        let cloned_executor = executor.clone();

        let wrap_state = api.clone();
        let closure = move |_caller: Caller, val| -> Result<(), wasmtime::Trap> {
            let transformed_val = {
                <CustomType as FromWasm<&Self::Wrap>>::from(
                    &wrap_state,
                    cloned_executor.as_ref(),
                    val,
                )?
            };

            let output = {
                let state = &mut wrap_state.lock().unwrap();
                cloned_executor.async_(Self::count_async(state, transformed_val))
            };

            let transformed_output = {
                    <CustomReturnType as ToWasm<&Self::Wrap>>::to(
                        &wrap_state,
                    cloned_executor.as_ref(),
                    output,
                )?
            };

            Ok(transformed_output)
        };

        linker.func("test", "test", closure).ok();
    }

    #[cfg(feature = "vm-wasmer")]
    fn add_to_wasmer_linker<E>(
        self,
        executor: E,
        linker: &mut crate::wasmer::WasmerLinker,
        store: &wasmer::Store,
    ) -> Self::Return
    where
        E: crate::Executor + Clone + 'static,
    {
        todo!()
    }
}
