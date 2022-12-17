use anyhow::{anyhow, Result};
use std::{fmt::Display, future::Future, pin::Pin};
use wasmtime::{Caller, Memory, Val};

// Get exported memory
pub fn get_memory<T>(caller: &mut Caller<T>) -> Result<Memory> {
    caller
        .get_export("memory")
        .or_trap("No export `memory` found")?
        .into_memory()
        .or_trap("Export `memory` is not a memory")
}

// Call guest to allocate a Vec of size `size`
pub fn allocate_guest_memory<'a, T: Send>(
    caller: &'a mut Caller<T>,
    size: u32,
    allocator_function_name: &'a str,
) -> Pin<Box<dyn Future<Output = Result<u32>> + Send + 'a>> {
    Box::pin(async move {
        let mut results = [Val::I32(0)];
        let result = caller
            .get_export(allocator_function_name)
            .or_trap("no export named alloc found")?
            .into_func()
            .or_trap("cannot turn export into func")?
            .call_async(caller, &[Val::I32(size as i32)], &mut results)
            .await;

        result.or_trap("failed to call alloc")?;
        Ok(results[0].unwrap_i32() as u32)
    })
}

pub trait IntoTrap<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T>;
}

impl<T, E: Display> IntoTrap<T> for Result<T, E> {
    fn or_trap<S: Display>(self, info: S) -> Result<T> {
        match self {
            Ok(result) => Ok(result),
            Err(error) => Err(anyhow!(
                "Trap raised during host call: {} ({}).",
                error,
                info
            )),
        }
    }
}

impl<T> IntoTrap<T> for Option<T> {
    fn or_trap<S: Display>(self, info: S) -> Result<T> {
        match self {
            Some(result) => Ok(result),
            None => Err(anyhow!(
                "Trap raised during host call: Expected `Some({})` got `None` ({}).",
                std::any::type_name::<T>(),
                info
            )),
        }
    }
}
