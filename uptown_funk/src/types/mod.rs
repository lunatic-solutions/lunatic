mod pointers;
mod primitive;
mod result;

pub use pointers::CReprWasmType;
pub use pointers::Pointer;
pub use result::HasOk;

use crate::{Executor, Trap};

pub trait FromWasm<State> {
    type From;

    fn from(state: State, executor: &impl Executor, from: Self::From) -> Result<Self, Trap>
    where
        Self: Sized;
}

pub trait FromWasmSimple: Sized {
    type From;

    fn from(v: Self::From) -> Self;
}

impl<S, T: FromWasmSimple> FromWasm<S> for T {
    type From = T::From;

    #[inline]
    fn from(_: S, _: &impl Executor, from: Self::From) -> Result<Self, Trap>
    where
        Self: Sized,
    {
        Ok(<Self as FromWasmSimple>::from(from))
    }
}

pub trait FromWasmDirect: Sized {}

impl<T: FromWasmDirect> FromWasmSimple for T {
    type From = Self;

    #[inline]
    fn from(v: Self::From) -> Self {
        v
    }
}

pub trait ToWasm<State> {
    type To;

    fn to(state: State, executor: &impl Executor, host_value: Self) -> Result<Self::To, Trap>;
}

pub trait ToWasmSimple: Sized {
    type To;

    fn to(v: Self) -> Self::To;
}

impl<S, T: ToWasmSimple> ToWasm<S> for T {
    type To = T::To;

    #[inline]
    fn to(_: S, _: &impl Executor, value: Self) -> Result<Self::To, Trap>
    where
        Self: Sized,
    {
        Ok(<Self as ToWasmSimple>::to(value))
    }
}

pub trait ToWasmDirect: Sized {}

impl<T: ToWasmDirect> ToWasmSimple for T {
    type To = Self;

    #[inline]
    fn to(v: Self) -> Self::To {
        v
    }
}
