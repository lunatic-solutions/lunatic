use anyhow::{anyhow, Result};
use hash_map_id::HashMapId;
use lunatic_common_api::{allocate_guest_memory, get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessConfigCtx;
use sqlite::{Connection, State, Statement};
use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};
use wasmtime::{Caller, Linker, Memory, ResourceLimiter};

use crate::wire_format::{BindList, DbError, SqliteError, SqliteRow, SqliteValue};

pub const SQLITE_ROW: u32 = 100;
pub const SQLITE_DONE: u32 = 101;

pub type SQLiteConnections = HashMapId<Arc<Mutex<Connection>>>;
pub type SQLiteResults = HashMapId<Vec<u8>>;
// sometimes we need to lookup the connection_id for the statement
pub type SQLiteStatements = HashMapId<(u64, Statement)>;
// maps connection_id to name of allocation function
pub type SQLiteGuestAllocators = HashMap<u64, String>;
pub trait SQLiteCtx {
    fn sqlite_results(&self) -> &SQLiteResults;
    fn sqlite_results_mut(&mut self) -> &mut SQLiteResults;

    fn sqlite_connections(&self) -> &SQLiteConnections;
    fn sqlite_connections_mut(&mut self) -> &mut SQLiteConnections;

    fn sqlite_guest_allocator(&self) -> &SQLiteGuestAllocators;
    fn sqlite_guest_allocator_mut(&mut self) -> &mut SQLiteGuestAllocators;

    fn sqlite_statements(&self) -> &SQLiteStatements;
    fn sqlite_statements_mut(&mut self) -> &mut SQLiteStatements;
}

// Register the SqlLite apis
pub fn register<T: SQLiteCtx + ProcessState + Send + ErrorCtx + ResourceLimiter + Sync + 'static>(
    linker: &mut Linker<T>,
) -> Result<()>
where
    T::Config: lunatic_process_api::ProcessConfigCtx,
{
    linker.func_wrap("lunatic::sqlite", "open", open)?;
    linker.func_wrap("lunatic::sqlite", "query_prepare", query_prepare)?;
    linker.func_wrap(
        "lunatic::sqlite",
        "query_prepare_and_consume",
        query_prepare_and_consume,
    )?;
    linker.func_wrap("lunatic::sqlite", "query_result_get", query_result_get)?;
    linker.func_wrap("lunatic::sqlite", "drop_query_result", drop_query_result)?;
    linker.func_wrap("lunatic::sqlite", "execute", execute)?;
    linker.func_wrap("lunatic::sqlite", "bind_value", bind_value)?;
    linker.func_wrap("lunatic::sqlite", "sqlite3_changes", sqlite3_changes)?;
    linker.func_wrap("lunatic::sqlite", "statement_reset", statement_reset)?;
    linker.func_wrap2_async("lunatic::sqlite", "last_error", last_error)?;
    linker.func_wrap("lunatic::sqlite", "sqlite3_finalize", sqlite3_finalize)?;
    linker.func_wrap("lunatic::sqlite", "sqlite3_step", sqlite3_step)?;
    linker.func_wrap3_async("lunatic::sqlite", "read_column", read_column)?;
    linker.func_wrap2_async("lunatic::sqlite", "column_names", column_names)?;
    linker.func_wrap2_async("lunatic::sqlite", "read_row", read_row)?;
    linker.func_wrap("lunatic::sqlite", "column_count", column_count)?;
    linker.func_wrap3_async("lunatic::sqlite", "column_name", column_name)?;
    Ok(())
}

fn open<T>(
    mut caller: Caller<T>,
    path_str_ptr: u32,
    path_str_len: u32,
    connection_id_ptr: u32,
) -> Result<u64>
where
    T: ProcessState + ErrorCtx + SQLiteCtx,
    T::Config: lunatic_process_api::ProcessConfigCtx,
{
    // obtain the memory and the state
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // obtain the path as a byte slice reference
    let path = memory_slice
        .get(path_str_ptr as usize..(path_str_ptr + path_str_len) as usize)
        .or_trap("lunatic::sqlite::open")?;
    let path = std::str::from_utf8(path).or_trap("lunatic::sqlite::open")?;
    if !state.config().can_access_fs_location(Path::new(path)) {
        let error_id = state.error_resources_mut().add(
            anyhow::Error::msg("Permission denied").context(format!("Failed to access '{}'", path)),
        );
        memory
            .write(
                &mut caller,
                connection_id_ptr as usize,
                &error_id.to_le_bytes(),
            )
            .or_trap("lunatic::sqlite::open")?;
        return Ok(1);
    }

    // call the open function, and define the return code
    let (conn_or_err_id, return_code) = match sqlite::open(path) {
        Ok(conn) => (
            caller
                .data_mut()
                .sqlite_connections_mut()
                .add(Arc::new(Mutex::new(conn))),
            0,
        ),
        Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
    };

    // write the result into memory and return the return code
    memory
        .write(
            &mut caller,
            connection_id_ptr as usize,
            &conn_or_err_id.to_le_bytes(),
        )
        .or_trap("lunatic::sqlite::open")?;
    Ok(return_code)
}

fn execute<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
    exec_str_ptr: u32,
    exec_str_len: u32,
) -> Result<u32> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let exec = memory_slice
        .get(exec_str_ptr as usize..(exec_str_ptr + exec_str_len) as usize)
        .or_trap("lunatic::sqlite::execute")?;
    let exec = std::str::from_utf8(exec).or_trap("lunatic::sqlite::execute")?;

    // execute a single sqlite query
    match state
        .sqlite_connections()
        .get(conn_id)
        .or_trap("lunatic::sqlite::execute")?
        .lock()
        .or_trap("lunatic::sqlite::execute")?
        .execute(exec)
    {
        // 1 is equal to SQLITE_ERROR, which is a generic error code
        Err(e) => Ok(e.code.unwrap_or(1) as u32),
        Ok(_) => Ok(0),
    }
}

fn query_prepare<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
    query_str_ptr: u32,
    query_str_len: u32,
) -> Result<u64> {
    // get the memory
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // get the query
    let query = memory_slice
        .get(query_str_ptr as usize..(query_str_ptr + query_str_len) as usize)
        .or_trap("lunatic::sqlite::query_prepare::get_query")?;
    let query = std::str::from_utf8(query).or_trap("lunatic::sqlite::query_prepare::from_utf8")?;

    let statement = {
        // obtain the sqlite connection
        let conn = state
            .sqlite_connections()
            .get(conn_id)
            .take()
            .or_trap("lunatic::sqlite::query_prepare::obtain_conn")?
            .lock()
            .or_trap("lunatic::sqlite::query_prepare::obtain_conn")?;

        // prepare the statement
        conn.prepare(query)
            .or_trap("lunatic::sqlite::query_prepare::prepare_statement")?
    };

    let statement_id = state.sqlite_statements_mut().add((conn_id, statement));

    Ok(statement_id)
}

macro_rules! get_statement {
    ($state:ident, $statement_id:ident) => {
        $state
            .sqlite_statements_mut()
            .get_mut($statement_id)
            .map(|(connection_id, statement)| (*connection_id, statement))
            .or_trap("lunatic::sqlite::get_statement_by_id")?
    };
}

macro_rules! get_conn {
    ($state:ident, $conn_id:ident, $fn_name:literal) => {{
        let trap_str = format!("lunatic::sqlite::{}::obtain_conn", $fn_name);
        $state
            .sqlite_connections_mut()
            .get($conn_id)
            .take()
            .or_trap(&trap_str)?
            .lock()
            .or_trap(trap_str)?
    }};
}

fn bind_value<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    statement_id: u64,
    bind_data_ptr: u32,
    bind_data_len: u32,
) -> Result<()> {
    // get the memory
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    let (_, statement) = get_statement!(state, statement_id);

    // get the query
    let bind_data = memory_slice
        .get(bind_data_ptr as usize..(bind_data_ptr + bind_data_len) as usize)
        .or_trap("lunatic::sqlite::bind_value::load_bind_data")?;

    let values: BindList = bincode::deserialize(bind_data).unwrap();

    for pair in values.iter() {
        pair.bind(statement)
            .or_trap("lunatic::sqlite::bind_value")?;
    }

    Ok(())
}

fn query_prepare_and_consume<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
    query_str_ptr: u32,
    query_str_len: u32,
    len_ptr: u32,
) -> Result<u64> {
    // get the memory
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // get the query
    let query = memory_slice
        .get(query_str_ptr as usize..(query_str_ptr + query_str_len) as usize)
        .or_trap("lunatic::sqlite::query_prepare::get_query")?;
    let query = std::str::from_utf8(query).or_trap("lunatic::sqlite::query_prepare::from_utf8")?;

    // allocate a vec to return some bytes back to the module
    let mut return_value = Vec::new();
    {
        // obtain the sqlite connection
        let conn = state
            .sqlite_connections()
            .get(conn_id)
            .or_trap("lunatic::sqlite::query_prepare::obtain_conn")?
            .lock()
            .or_trap("lunatic::sqlite::query_prepare::obtain_conn")?;

        // prepare the statement
        let mut statement = conn
            .prepare(query)
            .or_trap("lunatic::sqlite::query_prepare::prepare_statement")?;

        while let Ok(sqlite::State::Row) = statement.next() {
            let count = statement.column_count();
            for i in 0..count {
                match statement.column_type(i) {
                    Ok(sqlite::Type::Binary) => {
                        let mut bytes = statement
                            .read::<Vec<u8>, usize>(i)
                            .or_trap("lunatic::sqlite::query_prepare::read_binary")?;

                        let len = bytes.len();
                        return_value.append(&mut vec![ColumnType::Binary as u8]);
                        return_value.append(&mut (len as u32).to_le_bytes().to_vec());
                        return_value.append(&mut bytes);
                    }
                    Ok(sqlite::Type::Float) => {
                        return_value.append(&mut vec![ColumnType::Float as u8]);
                        return_value.append(
                            &mut statement
                                .read::<f64, usize>(i)
                                .or_trap("lunatic::sqlite::query_prepare::read_float")?
                                .to_le_bytes()
                                .to_vec(),
                        );
                    }
                    Ok(sqlite::Type::Integer) => {
                        return_value.append(&mut vec![ColumnType::Integer as u8]);
                        return_value.append(
                            &mut statement
                                .read::<i64, usize>(i)
                                .or_trap("lunatic::sqlite::query_prepare::read_integer")?
                                .to_le_bytes()
                                .to_vec(),
                        );
                    }
                    Ok(sqlite::Type::String) => {
                        let bytes = statement
                            .read::<String, usize>(i)
                            .or_trap("lunatic::sqlite::query_prepare::read_string")?;

                        let len = bytes.len();
                        return_value.append(&mut vec![ColumnType::String as u8]);
                        return_value.append(&mut (len as u32).to_le_bytes().to_vec());
                        return_value.append(&mut bytes.as_bytes().to_vec());
                    }
                    Ok(sqlite::Type::Null) => {
                        return_value.append(&mut vec![ColumnType::Null as u8])
                    }
                    Err(_e) => {}
                };
            }

            return_value.push(ColumnType::NewRow as u8);
        }
    }

    // write length into memory
    let mut slice = memory_slice
        .get_mut(len_ptr as usize..(len_ptr as usize + 8))
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;
    slice
        .write(&(return_value.len() as u64).to_le_bytes())
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;

    // store the result of the query
    let result_id = state.sqlite_results_mut().add(return_value);

    Ok(result_id)
}

fn query_result_get<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    resource_id: u64,
    data_ptr: u32,
    data_len: u32,
) -> Result<()> {
    // get the memory and the state
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // get the vector
    let result = state
        .sqlite_results()
        .get(resource_id)
        .or_trap("lunatic::sqlite::query_result_get::get_result")?;

    memory_slice
        .get_mut(data_ptr as usize..(data_ptr + data_len) as usize)
        .or_trap("lunatic::sqlite::query_result_get::write_result")?
        .write(result)
        .or_trap("lunatic::sqlite::query_result_get::write_result")?;

    Ok(())
}

fn drop_query_result<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    result_id: u64,
) -> Result<()> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);

    let results = state.sqlite_results_mut();
    results
        .remove(result_id)
        .or_trap("lunatic::sqlite::drop_query_result")?;

    Ok(())
}

fn sqlite3_changes<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
) -> Result<u32> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);
    let conn = get_conn!(state, conn_id, "sqlite3_changes");

    Ok(conn.change_count() as u32)
}

fn statement_reset<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    statement_id: u64,
) -> Result<()> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);
    let (_, stmt) = get_statement!(state, statement_id);

    stmt.reset().or_trap("lunatic::sqlite::statement_reset")?;

    Ok(())
}

// return a u64 which contains both the length of the pointer (usize=u32) and the pointer itself (u32)
async fn write_to_guest_vec<T: ProcessState + ErrorCtx + SQLiteCtx + Send + Sync>(
    mut caller: Caller<'_, T>,
    _connection_id: u64,
    memory: Memory,
    encoded_vec: Vec<u8>,
    opaque_ptr: u32,
) -> Result<u32> {
    let alloc_len = encoded_vec.len();
    let alloc_ptr = {
        let alloc_ptr = allocate_guest_memory(&mut caller, alloc_len as u32)
            .await
            .or_trap("lunatic::sqlite::write_to_guest_vec::alloc_response_vec")?;

        let (memory_slice, _) = memory.data_and_store_mut(&mut caller);
        let mut alloc_vec = memory_slice
            .get_mut(alloc_ptr as usize..(alloc_ptr as usize + alloc_len))
            .or_trap("lunatic::sqlite::write_to_guest_vec")?;

        alloc_vec
            .write(&encoded_vec)
            .or_trap("lunatic::sqlite::write_to_guest_vec")?;

        alloc_ptr
    };

    memory
        .write(&mut caller, opaque_ptr as usize, &alloc_len.to_le_bytes())
        .or_trap("lunatic::networking::tcp_read")?;
    Ok(alloc_ptr as u32)
}

fn read_column<T: ProcessState + ErrorCtx + SQLiteCtx + Send + Sync>(
    mut caller: Caller<T>,
    statement_id: u64,
    col_idx: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        // get state
        let memory = get_memory(&mut caller)?;
        let (_, state) = memory.data_and_store_mut(&mut caller);
        let (connection_id, stmt) = get_statement!(state, statement_id);

        let column = bincode::serialize(&SqliteValue::read_column(stmt, col_idx as usize)?)
            .or_trap("lunatic::sqlite::read_column")?;

        write_to_guest_vec(caller, connection_id, memory, column, opaque_ptr).await
    })
}

fn column_names<T: ProcessState + ErrorCtx + SQLiteCtx + Send + Sync>(
    mut caller: Caller<T>,
    statement_id: u64,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        // get state
        let memory = get_memory(&mut caller)?;
        let (_, state) = memory.data_and_store_mut(&mut caller);
        let (connection_id, stmt) = get_statement!(state, statement_id);

        let column_names = stmt.column_names().to_vec();

        let column_names =
            bincode::serialize(&column_names).or_trap("lunatic::sqlite::column_names")?;

        write_to_guest_vec(caller, connection_id, memory, column_names, opaque_ptr).await
    })
}

// this function assumes that the row has not been read yet and therefore
// starts at column_idx 0
fn read_row<T: ProcessState + ErrorCtx + SQLiteCtx + Send + Sync>(
    mut caller: Caller<T>,
    statement_id: u64,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        // get state
        let memory = get_memory(&mut caller)?;
        let (_, state) = memory.data_and_store_mut(&mut caller);
        let (connection_id, stmt) = get_statement!(state, statement_id);

        let read_row = SqliteRow::read_row(stmt)?;

        let row = bincode::serialize(&read_row).or_trap("lunatic::sqlite::read_row")?;

        write_to_guest_vec(caller, connection_id, memory, row, opaque_ptr).await
    })
}

fn last_error<T: ProcessState + ErrorCtx + SQLiteCtx + ResourceLimiter + Send + Sync>(
    mut caller: Caller<T>,
    conn_id: u64,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        // get state
        let memory = get_memory(&mut caller)?;
        let err = {
            let (_, state) = memory.data_and_store_mut(&mut caller);
            let mut conn = get_conn!(state, conn_id, "last_error");

            let err: SqliteError = conn.last().or_trap("lunatic::sqlite::last_error")?.into();
            bincode::serialize(&err)
                .or_trap("lunatic::sqlite::last_error::encode_error_wire_format")?
        };

        write_to_guest_vec(caller, conn_id, memory, err, opaque_ptr).await
    })
}

fn sqlite3_finalize<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    statement_id: u64,
) -> Result<()> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);
    // dropping the statement should invoke the C function `sqlite3_finalize`
    state
        .sqlite_statements_mut()
        .remove(statement_id)
        .or_trap("lunatic::sqlite::sqlite3_finalize")?;

    Ok(())
}

// sends back SQLITE_DONE or SQLITE_ROW depending on whether there's more data available or not
fn sqlite3_step<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    statement_id: u64,
) -> Result<u32> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);
    let (_, statement) = get_statement!(state, statement_id);

    match statement.next().or_trap("lunatic::sqlite::sqlite3_step")? {
        State::Done => Ok(SQLITE_DONE),
        State::Row => Ok(SQLITE_ROW),
    }
}

fn column_count<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    statement_id: u64,
) -> Result<u32> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);
    let (_, statement) = get_statement!(state, statement_id);

    Ok(statement.column_count() as u32)
}

fn column_name<T: ProcessState + ErrorCtx + SQLiteCtx + Send + Sync>(
    mut caller: Caller<T>,
    statement_id: u64,
    column_idx: u32,
    opaque_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_> {
    Box::new(async move {
        // get state
        let memory = get_memory(&mut caller)?;
        let (connection_id, column_name) = {
            let (_, state) = memory.data_and_store_mut(&mut caller);
            let (connection_id, statement) = get_statement!(state, statement_id);

            (
                connection_id,
                statement
                    .column_name(column_idx as usize)
                    .or_trap("lunatic::sqlite::column_name")?
                    .to_owned(),
            )
        };

        write_to_guest_vec(
            caller,
            connection_id,
            memory,
            column_name.into_bytes(),
            opaque_ptr,
        )
        .await
    })
}

#[repr(u8)]
enum ColumnType {
    Binary = 0x00,  // has 4 bytes of length header, followed by the bytes
    Float = 0x01,   // occupies 8 bytes f64
    Integer = 0x02, // occupies 8 bytes i64
    String = 0x03,  // has 4 bytes of length header, followed by the bytes
    Null = 0x04,    // has no variable header, in fact occupies only single byte
    NewRow = 0x05,  // indicates end of the row
}
