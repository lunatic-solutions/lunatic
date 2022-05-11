use anyhow::Result;
use std::fmt::Display;
use wasmtime::{Caller, Memory, Trap};

// Get exported memory
pub fn get_memory<T>(caller: &mut Caller<T>) -> std::result::Result<Memory, Trap> {
    caller
        .get_export("memory")
        .or_trap("No export `memory` found")?
        .into_memory()
        .or_trap("Export `memory` is not a memory")
}

pub trait IntoTrap<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap>;
}

impl<T, E: Display> IntoTrap<T> for Result<T, E> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap> {
        match self {
            Ok(result) => Ok(result),
            Err(error) => Err(Trap::new(format!(
                "Trap raised during host call: {} ({}).",
                error, info
            ))),
        }
    }
}

impl<T> IntoTrap<T> for Option<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T, Trap> {
        match self {
            Some(result) => Ok(result),
            None => Err(Trap::new(format!(
                "Trap raised during host call: Expected `Some({})` got `None` ({}).",
                std::any::type_name::<T>(),
                info
            ))),
        }
    }
}
