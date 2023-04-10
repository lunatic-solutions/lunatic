use std::{fmt::Display, future::Future, io::Write, pin::Pin};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::OnceCell;
use wasmtime::{Caller, Memory, Val};

const ALLOCATOR_FUNCTION_NAME: &str = "lunatic_alloc";
const FREEING_FUNCTION_NAME: &str = "lunatic_free";

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
) -> Pin<Box<dyn Future<Output = Result<u32>> + Send + 'a>> {
    Box::pin(async move {
        let mut results = [Val::I32(0)];
        caller
            .get_export(ALLOCATOR_FUNCTION_NAME)
            .or_trap(format!("no export named {ALLOCATOR_FUNCTION_NAME} found"))?
            .into_func()
            .or_trap("cannot turn export into func")?
            .call_async(caller, &[Val::I32(size as i32)], &mut results)
            .await
            .or_trap(format!("failed to call {ALLOCATOR_FUNCTION_NAME}"))?;

        Ok(results[0]
            .i32()
            .or_trap(format!("result of {ALLOCATOR_FUNCTION_NAME} is not i32"))? as u32)
    })
}

// Call guest to free a slice of memory at location ptr
pub fn free_guest_memory<'a, T: Send>(
    caller: &'a mut Caller<T>,
    ptr: u32,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut results = [];
        let result = caller
            .get_export(FREEING_FUNCTION_NAME)
            .or_trap(format!("no export named {FREEING_FUNCTION_NAME} found"))?
            .into_func()
            .or_trap("cannot turn export into func")?
            .call_async(caller, &[Val::I32(ptr as i32)], &mut results)
            .await;

        result.or_trap(format!("failed to call {FREEING_FUNCTION_NAME}"))?;
        Ok(())
    })
}

// Allocates and writes data to guest memory, updating the len_ptr and returning the allocated ptr.
pub async fn write_to_guest_vec<T: Send>(
    caller: &mut Caller<'_, T>,
    memory: &Memory,
    data: &[u8],
    len_ptr: u32,
) -> Result<u32> {
    let alloc_len = data.len();
    let alloc_ptr = allocate_guest_memory(caller, alloc_len as u32).await?;

    let (memory_slice, _) = memory.data_and_store_mut(&mut (*caller));
    let mut alloc_vec = memory_slice
        .get_mut(alloc_ptr as usize..(alloc_ptr as usize + alloc_len))
        .context("allocated memory does not exist")?;

    alloc_vec.write_all(data)?;

    memory.write(caller, len_ptr as usize, &alloc_len.to_le_bytes())?;

    Ok(alloc_ptr)
}

pub trait MetricsExt<T> {
    fn with_current_context<F>(&self, f: F)
    where
        F: Fn(&T, opentelemetry::Context);
}

impl<T> MetricsExt<T> for OnceCell<T> {
    fn with_current_context<F>(&self, f: F)
    where
        F: Fn(&T, opentelemetry::Context),
    {
        if let Some(v) = self.get() {
            f(v, opentelemetry::Context::current());
        }
    }
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
