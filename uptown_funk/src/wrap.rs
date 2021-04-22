use std::{borrow::BorrowMut, pin::Pin, rc::Rc, sync::{Arc, RwLock}};

use crate::{FromWasm, HostFunctions, ToWasm};
use wasmtime::Caller;

#[derive(Debug, Default)]
pub struct State {
    counter: u32,
    log: Vec<u32>,
}

type CustomType = u32;
type CustomReturnType = ();

impl State {
    fn count(&mut self, val: CustomType) -> CustomReturnType {
        self.counter += val;
        self.log.push(val);
    }

    async fn count_async(&mut self, val: CustomType) -> CustomReturnType {
        self.counter += val;
        self.log.push(val);
    }
}

pub type Wrap<T> = Arc<RwLock<Box<T>>>;

impl HostFunctions for State {
    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(self, executor: E, linker: &mut wasmtime::Linker) -> Wrap<Self>
    where
        E: crate::Executor + Clone + 'static,
    {
        let executor = Rc::new(executor);
        let boxed = Box::new(self);
        //let pointer_self = &mut *boxed as *mut Self;
        let ret = Arc::new(RwLock::new(boxed));

        let cloned_executor = executor.clone();
        let wrap_state = ret.clone();
        let closure = move |caller: Caller, val| -> Result<(), wasmtime::Trap> {
            //let state = unsafe { std::mem::transmute::<*mut Self, &mut Self>(pointer_self) };

            // Transform all closure arguments with `FromWasm`
            // TODO state needs to be behind the lock
            let transformed_val = {
                let mut write_state = wrap_state.write().unwrap();

                let state = write_state.as_mut();
                <CustomType as FromWasm<&mut Self>>::from(
                    state,
                    cloned_executor.as_ref(),
                    val,
                )?
            };

            // lock read/write depending if &self or &mut self is required
            //let _lock = wrap_state.write().unwrap();
            // Wrap in `executor.async_` if async
            let output = {
                let mut write_state = wrap_state.write().unwrap();
                let state = write_state.as_mut();
                cloned_executor.async_(Self::count_async(state, transformed_val))
            };
            //drop(_lock);

            let transformed_output = {
                    let mut write_state = wrap_state.write().unwrap();
                    let state = write_state.as_mut();
                    <CustomReturnType as ToWasm<&mut Self>>::to(
                    state,
                    cloned_executor.as_ref(),
                    output,
                )?
            };

            Ok(transformed_output)
        };

        linker.func("test", "test", closure).ok();

        ret.clone()
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
