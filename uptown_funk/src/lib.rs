pub mod memory;
pub mod state;
pub mod types;
#[cfg(feature = "vm-wasmer")]
pub mod wasmer;
pub mod wrap;

use wrap::Wrap;

use std::{convert::Into};
use std::fmt::Debug;
use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

pub use smallvec::SmallVec;
pub use types::{FromWasm, ToWasm};
pub use uptown_funk_macro::host_functions;

/// Provides access to the instance execution environment.
pub trait Executor {
    /// Execute `Future` f.
    #[cfg(feature = "async")]
    fn async_<R, F>(&self, f: F) -> R
    where
        F: std::future::Future<Output = R>;

    /// Get mutable access to the instance memory.
    fn memory(&self) -> memory::Memory;
}

pub trait HostFunctions: Sized {
    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(api: Wrap<Self>, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static;

    #[cfg(feature = "vm-wasmer")]
    fn add_to_wasmer_linker<E>(
        self,
        executor: E,
        linker: &mut wasmer::WasmerLinker,
        store: &::wasmer::Store,
    ) -> Self::Return
    where
        E: Executor + Clone + 'static;
}

pub struct StateWrapper<S, E: Executor> {
    state: Rc<RefCell<S>>,
    env: Rc<E>,
}

impl<S, E: Executor> StateWrapper<S, E> {
    pub fn new(state: S, executor: E) -> Self {
        Self {
            state: Rc::new(RefCell::new(state)),
            env: Rc::new(executor),
        }
    }

    pub fn borrow_state(&self) -> Ref<S> {
        self.state.borrow()
    }

    pub fn borrow_state_mut(&self) -> RefMut<S> {
        self.state.borrow_mut()
    }

    pub fn get_state(&self) -> Rc<RefCell<S>> {
        self.state.clone()
    }

    pub fn executor(&self) -> &E {
        &self.env
    }

    pub fn memory(&self) -> memory::Memory {
        self.env.memory()
    }

    pub fn recover_state(self) -> Result<S, ()> {
        match Rc::try_unwrap(self.state) {
            Ok(s) => Ok(s.into_inner()),
            Err(_) => Err(()),
        }
    }
}

// TODO document these
#[cfg(feature = "vm-wasmer")]
unsafe impl<S, E: Executor> Send for StateWrapper<S, E> {}
#[cfg(feature = "vm-wasmer")]
unsafe impl<S, E: Executor> Sync for StateWrapper<S, E> {}

impl<S, E: Executor> Clone for StateWrapper<S, E> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            env: self.env.clone(),
        }
    }
}

#[cfg(feature = "vm-wasmer")]
impl<S, E: Executor> ::wasmer::WasmerEnv for StateWrapper<S, E> {
    fn init_with_instance(
        &mut self,
        _: &::wasmer::Instance,
    ) -> Result<(), ::wasmer::HostEnvInitError> {
        Ok(())
    }
}

#[cfg_attr(feature = "vm-wasmer", derive(thiserror::Error))]
#[cfg_attr(feature = "vm-wasmer", error("{message}"))]
pub struct Trap<D = ()>
where
    D: 'static,
{
    message: String,
    data: Option<D>,
}

impl<D> Debug for Trap<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.message, f)
    }
}

impl Trap<()> {
    pub fn new<I: Into<String>>(message: I) -> Self {
        Self {
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data<D: 'static>(self, data: D) -> Trap<D> {
        Trap {
            message: self.message,
            data: Some(data),
        }
    }

    pub fn try_option<R: Debug>(result: Option<R>) -> Result<R, Trap> {
        match result {
            Some(r) => Ok(r),
            None => Err(Trap::new(
                "Host function trapped: Memory location not inside wasm guest",
            )),
        }
    }

    pub fn try_result<R: Debug, E: Debug>(result: Result<R, E>) -> Result<R, Trap> {
        match result {
            Ok(r) => Ok(r),
            Err(_) => {
                let message = format!("Host function trapped: {:?}", result);
                Err(Trap::new(message))
            }
        }
    }
}

impl<S> ToWasm<S> for Trap {
    type To = ();

    fn to(_: S, _: &impl Executor, v: Self) -> Result<(), Trap> {
        Err(v)
    }
}

#[cfg(feature = "vm-wasmtime")]
impl From<Trap> for wasmtime::Trap {
    fn from(trap: Trap) -> Self {
        wasmtime::Trap::new(trap.message)
    }
}

#[repr(C)]
pub struct IoVecT {
    pub ptr: u32,
    pub len: u32,
}
