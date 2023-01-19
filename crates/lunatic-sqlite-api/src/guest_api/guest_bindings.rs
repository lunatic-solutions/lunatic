pub use crate::wire_format::{
    BindKey, BindList, BindPair, BindValue, SqliteError, SqliteRow, SqliteValue,
};

pub mod sqlite_guest_bindings {
    #[link(wasm_import_module = "lunatic::sqlite")]
    extern "C" {
        /// opens a new connection and stores a reference to the resource
        ///
        /// returns the connection_id which can be used for later calls and
        /// can be safely transported between guest and host
        pub fn open(path: *const u8, path_len: usize, connection_id: *mut u32) -> u64;

        ///
        /// Creates a new prepared statement and returns the id of the prepared statement
        /// to the guest so that values can be bound to the statement at a later point
        pub fn query_prepare(connection_id: u64, query_str: *const u8, query_str_len: u32) -> u64;

        /// Creates a new statement and immediately reads the rows and keeps them in the
        /// host.
        /// The length of data is written into `len_ptr` so that it's possible to allocate
        /// the correct amount of memory in the guest for further reading of results.
        ///
        /// Returns a resource_id that can be used to read the results with
        /// `query_results_get` as well as dropped with `drop_query_result`
        pub fn query_prepare_and_consume(
            connection_id: u64,
            query_str: *const u8,
            query_str_len: u32,
            len_ptr: *mut u32,
        ) -> u64;

        /// Writes data from a previous query result (identified by `resource_id`)
        /// into the provided buffer
        pub fn query_result_get(resource_id: u64, write_buf: *const u8, write_buf_len: u32);

        /// Drops the results of a previous query result (identified by `resource_id`)
        /// from the host
        pub fn drop_query_result(resource_id: u64);

        /// Executes the passed query and returns the SQLite response code
        pub fn execute(connection_id: u64, exec_str: *const u8, exec_str_len: u32) -> u32;

        /// Binds one or more values to the statement identified by `statement_id`.
        /// The function expects to receive a `BindList` encoded via `bincode` as demonstrated by this example:
        ///
        /// ```
        ///
        /// let query = "INSERT INTO cars (manufacturer, model) VALUES(:manufacturer, :model_name);"
        /// let statement_id = unsafe {sqlite_guest_bindings::query_prepare(connection_id, query.as_ptr(), query.len() as u32)};
        ///
        /// let key = BindKey::String("model_name".into());
        /// let value = BindValue::Int(996);
        /// let bind_list = BindList(vec![
        ///     BindPair(key, value)
        /// ]);
        /// let encoded = bincode::serialize(&bind_list).unwrap();
        /// let result = unsafe {
        ///     sqlite_guest_bindings::bind_value(
        ///         statement_id,
        ///         encoded.as_ptr() as u32,
        ///         encoded.len() as u32,
        ///     )
        /// };
        /// ```
        ///
        /// Anything other than a `BindList` will be rejected and a Trap will be returned
        pub fn bind_value(statement_id: u64, bind_data_ptr: u32, bind_data_len: u32);

        /// returns count of changes/rows that the last call to SQLite triggered
        pub fn sqlite3_changes(connection_id: u64) -> u32;

        /// resets the bound statement so that it can be used/bound again
        pub fn statement_reset(statement_id: u64);

        /// furthers the internal SQLite cursor and returns either
        /// SQLITE_DONE or SQLITE_ROW to indicate whether there's more
        /// data to be pulled from the previous query
        pub fn sqlite3_step(connection_id: u64) -> u32;

        /// Drops the connection identified by `connection_id` in the host and
        /// closes the connection to SQLite
        pub fn sqlite3_finalize(connection_id: u64);

        /// returns the count of columns for the executed statement
        pub fn column_count(statement_id: u64) -> u32;

        /// NOTE: the following functions will require a registered `alloc` function
        /// because it relies on calling into the guest and allocating a chunk of memory
        /// in the guest so that results of queries can be written directly into the
        /// guest memory and not temporarily stored in the host as is the case with
        /// `query_prepare_and_consume` and `query_result_get`.
        ///
        /// The functions have a return value of `u64` which contains the pointer
        /// to the allocated guest memory (most likely a `Vec<u8>`) to which the
        /// results of the call have been written.
        /// The value of `u64` is split into two `u32` parts respectively:
        /// - the length of data written
        /// - the pointer to the data
        ///
        /// and can be retrieved via a function such as this:
        ///
        /// ```
        /// fn unroll_vec(ptr: u64) -> Vec<u8> {    
        ///     unsafe {
        ///         // the leftmost half contains the length
        ///         let length = (ptr >> 32) as usize;
        ///         // the rightmost half contains the pointer
        ///         let ptr = 0x00000000FFFFFFFF & ptr;
        ///         Vec::from_raw_parts(ptr as *mut u8, length, length)
        ///     }
        /// }
        /// ```
        ///
        ///

        /// looks up the value of the last error, encodes an `SqliteError` via bincode
        /// and writes it into a guest allocated Vec<u8>
        /// Returns a composite length + pointer to the data (see explanation above)
        pub fn last_error(connection_id: u64, opaque_ptr: *mut u32) -> u32;

        /// reads the column under index `col_idx` encodes a `SqliteValue` via bincode
        /// and writes it into a guest allocated Vec<u8>
        /// Returns a composite length + pointer to the data (see explanation above)
        pub fn read_column(statement_id: u64, col_idx: u32, opaque_ptr: *mut u32) -> u32;

        /// reads the next row, encodes a `SqliteRow` via bincode
        /// and writes it into a guest allocated Vec<u8>
        pub fn read_row(statement_id: u64, opaque_ptr: *mut u32) -> u32;

        /// looks up the name of the column under index `col_idx`, encodes a `String` via bincode
        /// and writes it into a guest allocated Vec<u8>
        pub fn column_name(statement_id: u64, col_idx: u32, opaque_ptr: *mut u32) -> u32;

        /// looks up the value of the last error, encodes a `Vec<String>` via bincode
        /// and writes it into a guest allocated Vec<u8>
        pub fn column_names(statement_id: u64, opaque_ptr: *mut u32) -> u32;
    }
}
