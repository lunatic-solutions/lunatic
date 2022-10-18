use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;
use lunatic_process::state::ProcessState;
use sqlite::Connection;
use std::sync::Mutex;
use wasmtime::{Caller, Linker, Trap};

pub type SQLiteResource = HashMapId<Mutex<Connection>>;

pub trait SQLiteCtx {
    fn sqlite_resources(&self) -> &SQLiteResource;
    fn sqlite_resources_mut(&mut self) -> &mut SQLiteResource;
}

// Register the SqlLite apis
pub fn register<T: SQLiteCtx + ProcessState + Send + ErrorCtx + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap("lunatic::sqlite", "open", open)?;
    Ok(())
}

fn open<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    path_str_ptr: u32,
    path_str_len: u32,
    connection_id_ptr: u32,
) -> Result<u32, Trap> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, _state) = memory.data_and_store_mut(&mut caller);
    let path = memory_slice
        .get(path_str_ptr as usize..(path_str_ptr + path_str_len) as usize)
        .or_trap("lunatic::registry::put")?;
    let path = std::str::from_utf8(path).or_trap("lunatic::registry::put")?;

    let (conn_or_err_id, return_code) = match sqlite::open(path) {
        Ok(conn) => (
            caller
                .data_mut()
                .sqlite_resources_mut()
                .add(Mutex::new(conn)),
            0,
        ),
        Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
    };

    let memory = get_memory(&mut caller)?;
    memory
        .write(
            &mut caller,
            conn_or_err_id as usize,
            &connection_id_ptr.to_le_bytes(),
        )
        .or_trap("lunatic::sqlite::open")?;
    Ok(return_code)
}
