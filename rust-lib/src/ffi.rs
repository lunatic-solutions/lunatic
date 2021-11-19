pub mod error {
    #[link(wasm_import_module = "lunatic::error")]
    extern "C" {
        pub fn string_size(error_id: u64) -> u32;
        pub fn to_string(error_id: u64, error_str: *mut u8);
        pub fn drop(error_id: u64);
    }
}

pub mod message {
    #[link(wasm_import_module = "lunatic::message")]
    extern "C" {
        pub fn write_data(data: *const u8, data_len: usize) -> usize;
        pub fn push_process(process_id: u64) -> u64;
        pub fn push_tcp_stream(tcp_stream_id: u64) -> u64;
        pub fn send(process_id: u64, reply_handle: u64);

        pub fn receive(reply_handle: u64, timeout: u32) -> u32;
        pub fn read_data(data: *mut u8, data_len: usize) -> usize;
        #[allow(dead_code)]
        pub fn seek_data(position: u64);
        #[allow(dead_code)]
        pub fn data_size() -> u64;
        pub fn take_process(index: u64) -> u64;
        pub fn take_tcp_stream(index: u64) -> u64;
    }
}

pub mod process {
    #[link(wasm_import_module = "lunatic::process")]
    extern "C" {
        pub fn create_config(max_memory: u64, max_fuel: u64) -> u64;
        pub fn drop_config(config_id: u64);
        pub fn allow_namespace(config_id: u64, name: *const u8, name_len: usize);
        pub fn preopen_dir(config_id: u64, dir: *const u8, dir_len: usize, id: *mut u64) -> u32;
        pub fn add_plugin(
            config_id: u64,
            plugin_data: *const u8,
            plugin_data_len: usize,
            id: *mut u64,
        ) -> u32;
        pub fn create_environment(config_id: u64, id: *mut u64) -> u32;
        pub fn drop_environment(env_id: u64);
        pub fn add_module(
            env_id: u64,
            module_data: *const u8,
            module_data_len: usize,
            id: *mut u64,
        ) -> u32;
        pub fn add_this_module(env_id: u64, id: *mut u64) -> u32;
        pub fn drop_module(mod_id: u64);
        pub fn spawn(
            link: i64,
            module_id: u64,
            function: *const u8,
            function_len: usize,
            params: *const u8,
            params_len: usize,
            id: *mut u64,
        ) -> u32;
        pub fn inherit_spawn(
            link: i64,
            function: *const u8,
            function_len: usize,
            params: *const u8,
            params_len: usize,
            id: *mut u64,
        ) -> u32;
        pub fn drop_process(process_id: u64);
        pub fn clone_process(process_id: u64) -> u64;
        pub fn sleep_ms(millis: u64);
        pub fn die_when_link_dies(trap: u32);
        pub fn this() -> u64;
        pub fn id(process_id: u64, uuid: *mut [u8; 16]);
        pub fn this_env() -> u64;
        pub fn link(tag: i64, process_id: u64);
        pub fn unlink(process_id: u64);
        pub fn register(
            name: *const u8,
            name_len: usize,
            version: *const u8,
            version_len: usize,
            env_id: u64,
            process_id: u64,
        ) -> u32;
        pub fn unregister(
            name: *const u8,
            name_len: usize,
            version: *const u8,
            version_len: usize,
            env_id: u64,
        ) -> u32;
        pub fn lookup(
            name: *const u8,
            name_len: usize,
            query: *const u8,
            query_len: usize,
            id: *mut u64,
        ) -> u32;
    }
}
