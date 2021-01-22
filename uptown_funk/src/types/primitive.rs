use crate::{FromWasm, ToWasm, Trap};

impl FromWasm for u32 {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized {
        Ok(from)
    }
}

impl ToWasm for u32 {
    type To = u32;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        Ok(host_value)
    }
}

impl FromWasm for u64 {
    type From = u64;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized {
        Ok(from)
    }
}

impl ToWasm for u64 {
    type To = u64;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        Ok(host_value)
    }
}

impl FromWasm for u16 {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized {
        Ok(from as u16)
    }
}

impl ToWasm for u16 {
    type To = u32;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        Ok(host_value as u32)
    }
}

impl FromWasm for u8 {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized {
        Ok(from as u8)
    }
}

impl ToWasm for u8 {
    type To = u32;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        Ok(host_value as u32)
    }
}

impl ToWasm for bool {
    type To = u32;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl crate::Executor,
        host_value: Self,
    ) -> Result<Self::To, Trap> {
        Ok(if host_value { 0 } else { 1 })
    }
}