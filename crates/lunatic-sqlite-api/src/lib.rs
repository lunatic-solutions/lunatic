#[cfg(not(target_arch = "wasm32"))]
mod sqlite_bindings;

#[cfg(not(target_arch = "wasm32"))]
pub use sqlite_bindings::*;

#[cfg(target_arch = "wasm32")]
pub mod sqlite {
    #[link(wasm_import_module = "lunatic::sqlite")]
    extern "C" {
        pub fn open(path: *const u8, path_len: usize, conn_id: *mut u32) -> u64;
        pub fn query_prepare(
            conn_id: u64,
            query_str: *const u8,
            query_str_len: u32,
            len_ptr: *mut u32,
            resource_id: *mut u32,
        ) -> ();
        pub fn query_result_get(resource_id: u64, write_buf: *const u8, write_buf_len: u32) -> ();
        pub fn drop_query_result(resource_id: u64) -> ();
        pub fn execute(conn_id: u64, exec_str: *const u8, exec_str_len: u32) -> u32;
    }
}
