use anyhow::Result;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_process::state::ProcessState;
use lunatic_process_api::{ProcessConfigCtx, ProcessCtx};
use wasmtime::{Caller, Linker, ResourceLimiter, Trap};

// Register the process APIs to the linker
pub fn register<T>(linker: &mut Linker<T>) -> Result<()>
where
    T: ProcessState + ProcessCtx<T> + Send + ResourceLimiter + 'static,
    for<'a> &'a T: Send,
    T::Config: ProcessConfigCtx,
{
    linker.func_wrap("lunatic::distributed", "nodes_count", nodes_count)?;
    linker.func_wrap("lunatic::distributed", "get_nodes", get_nodes)?;
    Ok(())
}

// Returns count of registered nodes
fn nodes_count<T: ProcessState + ProcessCtx<T>>(_caller: Caller<T>) -> u32 {
    2
}

// Copy node ids to memory TODO doc
fn get_nodes<T: ProcessState + ProcessCtx<T>>(
    mut caller: Caller<T>,
    nodes_ptr: u32,
    nodes_len: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let test: Vec<u64> = vec![1, 2]; // TODO max nodes_len
    memory
        .data_mut(&mut caller)
        .get_mut(
            nodes_ptr as usize
                ..(nodes_ptr as usize + std::mem::size_of::<u64>() * nodes_len as usize),
        )
        .or_trap("lunatic::distributed::get_nodes::memory")?
        .copy_from_slice(unsafe { test.align_to::<u8>().1 });
    Ok(2)
}
