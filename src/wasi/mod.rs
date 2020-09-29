use wasmer::{Store, Function, Exports, ImportObject};

pub fn create_wasi_imports(store: Store, resolver: &mut ImportObject) {
    let mut wasi_env = Exports::new();

    fn proc_exit(index: i32) {
        println!("wasi_snapshot_preview1:proc_exit({}) called!", index);
        std::process::exit(index);
    }
    wasi_env.insert("proc_exit", Function::new_native(&store, proc_exit));

    fn fd_write(_: i32, _: i32, _: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:fd_write()");
        0
    }
    wasi_env.insert("fd_write", Function::new_native(&store, fd_write));


    fn fd_prestat_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:fd_prestat_get()");
        8 // WASI_EBADF
    }
    wasi_env.insert("fd_prestat_get", Function::new_native(&store, fd_prestat_get));

    fn fd_prestat_dir_name(_: i32, _: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:fd_prestat_dir_name()");
        28 // WASI_EINVAL
    }
    wasi_env.insert("fd_prestat_dir_name", Function::new_native(&store, fd_prestat_dir_name));


    fn environ_sizes_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:environ_sizes_get()");
        0
    }
    wasi_env.insert("environ_sizes_get", Function::new_native(&store, environ_sizes_get));

    fn environ_get(_: i32, _: i32) -> i32 {
        println!("wasi_snapshot_preview1:environ_get()");
        0
    }
    wasi_env.insert("environ_get", Function::new_native(&store, environ_get));

    resolver.register("wasi_snapshot_preview1", wasi_env);
}