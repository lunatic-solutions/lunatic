mod wire_format;

#[cfg(not(target_arch = "wasm32"))]
mod sqlite_bindings;

#[cfg(not(target_arch = "wasm32"))]
pub use sqlite_bindings::*;

#[cfg(target_arch = "wasm32")]
pub use wire_format::{
    BindKey, BindList, BindPair, BindValue, Parse, ParseStream, SqliteError, SqliteRow, SqliteValue,
};

pub mod sqlite_guest_bindings {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "lunatic::sqlite")]
    extern "C" {
        pub fn open(path: *const u8, path_len: usize, conn_id: *mut u32) -> u64;
        pub fn query_prepare(conn_id: u64, query_str: *const u8, query_str_len: u32) -> u64;
        pub fn query_prepare_and_consume(
            conn_id: u64,
            query_str: *const u8,
            query_str_len: u32,
            len_ptr: *mut u32,
            resource_id: *mut u32,
        );
        pub fn query_result_get(resource_id: u64, write_buf: *const u8, write_buf_len: u32);
        pub fn drop_query_result(resource_id: u64);
        pub fn execute(conn_id: u64, exec_str: *const u8, exec_str_len: u32) -> u32;
        pub fn bind_value(statement_id: u64, bind_data_ptr: u32, bind_data_len: u32);
        pub fn sqlite3_changes(conn_id: u64) -> u32;
        pub fn statement_reset(statement_id: u64);
        pub fn last_error(conn_id: u64) -> u64;
        pub fn sqlite3_finalize(conn_id: u64);
        pub fn sqlite3_step(conn_id: u64) -> u32;
        pub fn read_column(statement_id: u64, col_idx: u32) -> u64;
        pub fn read_row(statement_id: u64) -> u64;
        pub fn column_name(statement_id: u64, col_idx: u32) -> u64;
        pub fn column_count(statement_id: u64) -> u32;

        // returns a Vec<String> of column names
        pub fn column_names(statement_id: u64) -> u64;
    }
}

pub const SQLITE_OK: u32 = 0;
pub const SQLITE_ERROR: u32 = 1;
pub const SQLITE_INTERNAL: u32 = 2;
pub const SQLITE_PERM: u32 = 3;
pub const SQLITE_ABORT: u32 = 4;
pub const SQLITE_BUSY: u32 = 5;
pub const SQLITE_LOCKED: u32 = 6;
pub const SQLITE_NOMEM: u32 = 7;
pub const SQLITE_READONLY: u32 = 8;
pub const SQLITE_INTERRUPT: u32 = 9;
pub const SQLITE_IOERR: u32 = 10;
pub const SQLITE_CORRUPT: u32 = 11;
pub const SQLITE_NOTFOUND: u32 = 12;
pub const SQLITE_FULL: u32 = 13;
pub const SQLITE_CANTOPEN: u32 = 14;
pub const SQLITE_PROTOCOL: u32 = 15;
pub const SQLITE_EMPTY: u32 = 16;
pub const SQLITE_SCHEMA: u32 = 17;
pub const SQLITE_TOOBIG: u32 = 18;
pub const SQLITE_CONSTRAINT: u32 = 19;
pub const SQLITE_MISMATCH: u32 = 20;
pub const SQLITE_MISUSE: u32 = 21;
pub const SQLITE_NOLFS: u32 = 22;
pub const SQLITE_AUTH: u32 = 23;
pub const SQLITE_FORMAT: u32 = 24;
pub const SQLITE_RANGE: u32 = 25;
pub const SQLITE_NOTADB: u32 = 26;
pub const SQLITE_NOTICE: u32 = 27;
pub const SQLITE_WARNING: u32 = 28;
pub const SQLITE_ROW: u32 = 100;
pub const SQLITE_DONE: u32 = 101;
pub const SQLITE_OK_LOAD_PERMANENTLY: u32 = 256;
pub const SQLITE_ERROR_MISSING_COLLSEQ: u32 = 257;
pub const SQLITE_BUSY_RECOVERY: u32 = 261;
pub const SQLITE_LOCKED_SHAREDCACHE: u32 = 262;
pub const SQLITE_READONLY_RECOVERY: u32 = 264;
pub const SQLITE_IOERR_READ: u32 = 266;
pub const SQLITE_CORRUPT_VTAB: u32 = 267;
pub const SQLITE_CANTOPEN_NOTEMPDIR: u32 = 270;
pub const SQLITE_CONSTRAINT_CHECK: u32 = 275;
pub const SQLITE_AUTH_USER: u32 = 279;
pub const SQLITE_NOTICE_RECOVER_WAL: u32 = 283;
pub const SQLITE_WARNING_AUTOINDEX: u32 = 284;
pub const SQLITE_ERROR_RETRY: u32 = 513;
pub const SQLITE_ABORT_ROLLBACK: u32 = 516;
pub const SQLITE_BUSY_SNAPSHOT: u32 = 517;
pub const SQLITE_LOCKED_VTAB: u32 = 518;
pub const SQLITE_READONLY_CANTLOCK: u32 = 520;
pub const SQLITE_IOERR_SHORT_READ: u32 = 522;
pub const SQLITE_CORRUPT_SEQUENCE: u32 = 523;
pub const SQLITE_CANTOPEN_ISDIR: u32 = 526;
pub const SQLITE_CONSTRAINT_COMMITHOOK: u32 = 531;
pub const SQLITE_NOTICE_RECOVER_ROLLBACK: u32 = 539;
pub const SQLITE_ERROR_SNAPSHOT: u32 = 769;
pub const SQLITE_BUSY_TIMEOUT: u32 = 773;
pub const SQLITE_READONLY_ROLLBACK: u32 = 776;
pub const SQLITE_IOERR_WRITE: u32 = 778;
pub const SQLITE_CORRUPT_INDEX: u32 = 779;
pub const SQLITE_CANTOPEN_FULLPATH: u32 = 782;
pub const SQLITE_CONSTRAINT_FOREIGNKEY: u32 = 787;
pub const SQLITE_READONLY_DBMOVED: u32 = 1032;
pub const SQLITE_IOERR_FSYNC: u32 = 1034;
pub const SQLITE_CANTOPEN_CONVPATH: u32 = 1038;
pub const SQLITE_CONSTRAINT_FUNCTION: u32 = 1043;
pub const SQLITE_READONLY_CANTINIT: u32 = 1288;
pub const SQLITE_IOERR_DIR_FSYNC: u32 = 1290;
pub const SQLITE_CANTOPEN_DIRTYWAL: u32 = 1294;
pub const SQLITE_CONSTRAINT_NOTNULL: u32 = 1299;
pub const SQLITE_READONLY_DIRECTORY: u32 = 1544;
pub const SQLITE_IOERR_TRUNCATE: u32 = 1546;
pub const SQLITE_CANTOPEN_SYMLINK: u32 = 1550;
pub const SQLITE_CONSTRAINT_PRIMARYKEY: u32 = 1555;
pub const SQLITE_IOERR_FSTAT: u32 = 1802;
pub const SQLITE_CONSTRAINT_TRIGGER: u32 = 1811;
pub const SQLITE_IOERR_UNLOCK: u32 = 2058;
pub const SQLITE_CONSTRAINT_UNIQUE: u32 = 2067;
pub const SQLITE_IOERR_RDLOCK: u32 = 2314;
pub const SQLITE_CONSTRAINT_VTAB: u32 = 2323;
pub const SQLITE_IOERR_DELETE: u32 = 2570;
pub const SQLITE_CONSTRAINT_ROWID: u32 = 2579;
pub const SQLITE_IOERR_BLOCKED: u32 = 2826;
pub const SQLITE_CONSTRAINT_PINNED: u32 = 2835;
pub const SQLITE_IOERR_NOMEM: u32 = 3082;
pub const SQLITE_CONSTRAINT_DATATYPE: u32 = 3091;
pub const SQLITE_IOERR_ACCESS: u32 = 3338;
pub const SQLITE_IOERR_CHECKRESERVEDLOCK: u32 = 3594;
pub const SQLITE_IOERR_LOCK: u32 = 3850;
pub const SQLITE_IOERR_CLOSE: u32 = 4106;
pub const SQLITE_IOERR_DIR_CLOSE: u32 = 4362;
pub const SQLITE_IOERR_SHMOPEN: u32 = 4618;
pub const SQLITE_IOERR_SHMSIZE: u32 = 4874;
pub const SQLITE_IOERR_SHMLOCK: u32 = 5130;
pub const SQLITE_IOERR_SHMMAP: u32 = 5386;
pub const SQLITE_IOERR_SEEK: u32 = 5642;
pub const SQLITE_IOERR_DELETE_NOENT: u32 = 5898;
pub const SQLITE_IOERR_MMAP: u32 = 6154;
pub const SQLITE_IOERR_GETTEMPPATH: u32 = 6410;
pub const SQLITE_IOERR_CONVPATH: u32 = 6666;
pub const SQLITE_IOERR_VNODE: u32 = 6922;
pub const SQLITE_IOERR_AUTH: u32 = 7178;
pub const SQLITE_IOERR_BEGIN_ATOMIC: u32 = 7434;
pub const SQLITE_IOERR_COMMIT_ATOMIC: u32 = 7690;
pub const SQLITE_IOERR_ROLLBACK_ATOMIC: u32 = 7946;
pub const SQLITE_IOERR_DATA: u32 = 8202;
pub const SQLITE_IOERR_CORRUPTFS: u32 = 8458;
