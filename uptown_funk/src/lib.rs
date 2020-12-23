use std::convert::Into;
use std::fmt::Debug;

pub use smallvec::SmallVec;
pub use uptown_funk_macro::host_functions;

pub trait InstanceEnvironment {
    #[cfg(feature = "async")]
    fn async_<R, F>(&self, f: F) -> R
    where
        F: std::future::Future<Output = R>;
    fn wasm_memory(&self) -> &mut [u8];
}

pub trait HostFunctions {
    fn add_to_linker<E: 'static>(self, instance_environment: E, linker: &mut wasmtime::Linker)
    where
        E: InstanceEnvironment;
}

pub trait FromWasmI32 {
    type State;

    fn from_i32<I>(
        state: &Self::State,
        instance_environment: &I,
        wasm_i32: i32,
    ) -> Result<Self, Trap>
    where
        Self: Sized,
        I: InstanceEnvironment;
}

pub trait FromWasmI32Borrowed {
    type State;

    fn from_i32_borrowed<'a, I>(
        state: &'a Self::State,
        instance_environment: &I,
        wasm_i32: i32,
    ) -> Result<&'a Self, Trap>
    where
        I: InstanceEnvironment;
}

pub trait ToWasmI32 {
    type State;

    fn to_i32<I>(
        state: &Self::State,
        instance_environment: &I,
        host_value: Self,
    ) -> Result<i32, Trap>
    where
        I: InstanceEnvironment;
}

pub struct StateWrapper<S, E: InstanceEnvironment> {
    state: S,
    env: E,
}

impl<S, E: InstanceEnvironment> StateWrapper<S, E> {
    pub fn new(state: S, instance_environment: E) -> Self {
        Self {
            state: state,
            env: instance_environment,
        }
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn instance_environment(&self) -> &E {
        &self.env
    }

    pub fn wasm_memory(&self) -> &mut [u8] {
        self.env.wasm_memory()
    }
}

#[derive(Debug)]
pub struct Trap {
    message: String,
}

impl Trap {
    pub fn new<I: Into<String>>(message: I) -> Self {
        Self {
            message: message.into(),
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
