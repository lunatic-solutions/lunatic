use anyhow::Result;
use hash_map_id::HashMapId;
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_error_api::ErrorCtx;
use lunatic_process::state::ProcessState;
use sqlite::Connection;
use std::{io::Write, sync::Mutex};
use wasmtime::{Caller, Linker, Trap};

pub type SQLiteConnections = HashMapId<Mutex<Connection>>;
pub type SQLiteResults = HashMapId<Vec<u8>>;
pub trait SQLiteCtx {
    fn sqlite_results(&self) -> &SQLiteResults;
    fn sqlite_results_mut(&self) -> &mut SQLiteResults;

    fn sqlite_connections(&self) -> &SQLiteConnections;
    fn sqlite_connections_mut(&mut self) -> &mut SQLiteConnections;
}

// Register the SqlLite apis
pub fn register<T: SQLiteCtx + ProcessState + Send + ErrorCtx + 'static>(
    linker: &mut Linker<T>,
) -> Result<()> {
    linker.func_wrap("lunatic::sqlite", "open", open)?;
    linker.func_wrap("lunatic::sqlite", "query_prepare", query_prepare)?;
    linker.func_wrap("lunatic::sqlite", "query_result_get", query_result_get)?;
    linker.func_wrap("lunatic::sqlite", "drop_query_result", drop_query_result)?;
    linker.func_wrap("lunatic::sqlite", "execute", execute)?;
    Ok(())
}

fn open<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    path_str_ptr: u32,
    path_str_len: u32,
    connection_id_ptr: u32,
) -> Result<u32, Trap> {
    // obtain the memory and the state
    let memory = get_memory(&mut caller)?;
    let (memory_slice, _state) = memory.data_and_store_mut(&mut caller);

    // obtain the path as a byte slice reference
    let path = memory_slice
        .get(path_str_ptr as usize..(path_str_ptr + path_str_len) as usize)
        .or_trap("lunatic::sqlite::open")?;
    let path = std::str::from_utf8(path).or_trap("lunatic::sqlite::open")?;

    // call the open function, and define the return code
    let (conn_or_err_id, return_code) = match sqlite::open(path) {
        Ok(conn) => (
            caller
                .data_mut()
                .sqlite_connections_mut()
                .add(Mutex::new(conn)),
            0,
        ),
        Err(error) => (caller.data_mut().error_resources_mut().add(error.into()), 1),
    };

    // write the result into memory and return the return code
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

fn execute<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
    exec_str_ptr: u32,
    exec_str_len: u32,
) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);
    let exec = memory_slice
        .get(exec_str_ptr as usize..(exec_str_ptr + exec_str_len) as usize)
        .or_trap("lunatic::sqlite::execute")?;
    let exec = std::str::from_utf8(exec).or_trap("lunatic::sqlite::execute")?;

    // execute a single sqlite query
    state
        .sqlite_connections()
        .get(conn_id)
        .or_trap("lunatic::sqlite::execute")?
        .lock()
        .or_trap("lunatic::sqlite::execute")?
        .execute(exec)
        .or_trap("lunatic::sqlite::execute")
}

fn query_prepare<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    conn_id: u64,
    query_str_ptr: u32,
    query_str_len: u32,
    len_ptr: u32,
    resource_ptr: u32,
) -> Result<(), Trap> {
    // get the memory
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // get the query
    let query = memory_slice
        .get(query_str_ptr as usize..(query_str_ptr + query_str_len) as usize)
        .or_trap("lunatic::sqlite::query_prepare::get_query")?;
    let query = std::str::from_utf8(query).or_trap("lunatic::sqlite::query_prepare::from_utf8")?;

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

    // allocate a vec to return some bytes back to the module
    let mut return_value = Vec::new();

    while let Ok(sqlite::State::Row) = statement.next() {
        let count = statement.column_count();
        for i in 0..count {
            let mut bytes = match statement.column_type(i) {
                sqlite::Type::Binary => {
                    let mut bytes = statement
                        .read::<Vec<u8>>(i)
                        .or_trap("lunatic::sqlite::query_prepare::read_binary")?;

                    let len = bytes.len();
                    let mut result = vec![ColumnType::Binary as u8];
                    result.append(&mut (len as u32).to_le_bytes().to_vec());
                    result.append(&mut bytes);
                    result
                }
                sqlite::Type::Float => {
                    let mut result = vec![ColumnType::Float as u8];
                    result.append(
                        &mut statement
                            .read::<f64>(i)
                            .or_trap("lunatic::sqlite::query_prepare::read_float")?
                            .to_le_bytes()
                            .to_vec(),
                    );
                    result
                }
                sqlite::Type::Integer => {
                    let mut result = vec![ColumnType::Integer as u8];
                    result.append(
                        &mut statement
                            .read::<i64>(i)
                            .or_trap("lunatic::sqlite::query_prepare::read_integer")?
                            .to_le_bytes()
                            .to_vec(),
                    );
                    result
                }
                sqlite::Type::String => {
                    let bytes = statement
                        .read::<String>(i)
                        .or_trap("lunatic::sqlite::query_prepare::read_string")?;

                    let len = bytes.len();
                    let mut result = vec![ColumnType::String as u8];
                    result.append(&mut (len as u32).to_le_bytes().to_vec());
                    result.append(&mut bytes.as_bytes().to_vec());
                    result
                }
                sqlite::Type::Null => vec![ColumnType::Null as u8],
            };
            return_value.append(&mut bytes);
        }

        return_value.push(ColumnType::NewRow as u8);
    }

    // write length into memory
    let mut slice = memory_slice
        .get_mut(len_ptr as usize..(len_ptr as usize + 8 as usize))
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;
    slice
        .write(&(return_value.len() as u64).to_le_bytes())
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;

    // store the result of the query
    let results = state.sqlite_results_mut();
    let result_id = results.add(return_value);

    // write the result_id into memory
    let mut slice = memory_slice
        .get_mut(resource_ptr as usize..(resource_ptr as usize + 8))
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;

    slice
        .write(&result_id.to_le_bytes())
        .or_trap("lunatic::sqlite::query_prepare::write_memory")?;

    Ok(())
}

fn query_result_get<T: ProcessState + ErrorCtx + SQLiteCtx>(
    mut caller: Caller<T>,
    resource_id: u64,
    data_ptr: u32,
    data_len: u32,
) -> Result<(), Trap> {
    // get the memory and the state
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    // get the vevtor
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
) -> Result<(), Trap> {
    // get state
    let memory = get_memory(&mut caller)?;
    let (_, state) = memory.data_and_store_mut(&mut caller);

    let results = state.sqlite_results_mut();
    results
        .remove(result_id)
        .or_trap("lunatic::sqlite::drop_query_result")?;

    Ok(())
}

enum ColumnType {
    Binary = 0x00,  // has 4 bytes of length header, followed by the bytes
    Float = 0x01,   // occupies 8 bytes f64
    Integer = 0x02, // occupies 8 bytes i64
    String = 0x03,  // has 4 bytes of length header, followed by the bytes
    Null = 0x04,    // has no variable header, in fact occupies only single byte
    NewRow = 0x05,  // indicates end of the row
}
