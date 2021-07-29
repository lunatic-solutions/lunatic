use anyhow::Result;
use wasmtime::{Linker, Trap};

use super::namespace_matches_filter;
use crate::state::ProcessState;

// Register WASI APIs to the linker
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    // Add all WASI functions at first
    wasmtime_wasi::sync::snapshots::preview_1::add_wasi_snapshot_preview1_to_linker(
        linker,
        |ctx| &mut ctx.wasi,
    )?;

    // Override all functions not matched with a trap implementation.
    if !namespace_matches_filter("wasi_snapshot_preview1", "args_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "args_get", |_: i32, _: i32| -> Result<i32, Trap> {
            Err(Trap::new("Host function `wasi_snapshot_preview1::args_get` unavailable in this environment."))
        })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "args_sizes_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "args_sizes_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::args_sizes_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "clock_res_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "clock_res_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::clock_res_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "clock_time_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "clock_time_get", |_: i32, _:i64, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::clock_time_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "environ_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "environ_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::environ_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_advise", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_advise", |_: i32, _:i64, _:i64, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_advise` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_allocate", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_allocate", |_: i32, _:i64, _:i64| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_allocate` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_close", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_close",
            |_: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_close` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_datasync", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_datasync", |_: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_datasync` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_fdstat_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_fdstat_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_fdstat_set_flags",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_set_flags", |_: i32, _: i32| -> Result<i32, Trap> {
                Err(Trap::new("Host function `wasi_snapshot_preview1::fd_fdstat_set_flags` unavailable in this environment."))
            })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_fdstat_set_rights",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_fdstat_set_rights", |_: i32, _:i64, _:i64| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_fdstat_set_rights` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_filestat_get",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_filestat_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_filestat_set_size",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_set_size", |_: i32, _: i64| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_filestat_set_size` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_filestat_set_times",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_filestat_set_times", |_: i32, _:i64, _:i64, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_filestat_set_times` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_pread", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_pread",
            |_: i32, _: i32, _: i32, _: i64, _: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_pread` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fd_prestat_dir_name",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_prestat_dir_name", |_: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_prestat_dir_name` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_prestat_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_prestat_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_prestat_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_pwrite", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_pwrite", |_: i32, _: i32, _: i32, _: i64, _:i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_pwrite` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_read", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_read",
            |_: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_read` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_readdir", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_readdir", |_: i32, _: i32, _: i32, _: i64, _:i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_readdir` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_renumber", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "fd_renumber", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::fd_renumber` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_seek", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_seek",
            |_: i32, _: i64, _: i32, _: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_seek` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_sync", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_sync",
            |_: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_sync` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_tell", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_tell",
            |_: i32, _: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_tell` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "fd_write", namespace_filter) {
        linker.func_wrap(
            "wasi_snapshot_preview1",
            "fd_write",
            |_: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
                Err(Trap::new(
                "Host function `wasi_snapshot_preview1::fd_write` unavailable in this environment.",
            ))
            },
        )?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "fdpath_create_directory_tell",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "fdpath_create_directory_tell", |_: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_create_directory` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "path_filestat_get",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "path_filestat_get", |_: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_filestat_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "path_filestat_set_times",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "path_filestat_set_times", |_: i32, _: i32, _: i32, _: i32, _: i64, _: i64, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_filestat_set_times` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "path_link", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "path_link", |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_link` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "path_open", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "path_open", |_: i32, _: i32, _: i32, _: i32, _: i32, _: i64, _: i64, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_open` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "path_readlink", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "path_readlink", |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_readlink` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "path_remove_directory",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "path_remove_directory", |_: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_remove_directory` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "path_rename", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "path_rename", |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_rename` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "path_symlink", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "path_symlink", |_: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_symlink` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter(
        "wasi_snapshot_preview1",
        "path_unlink_file",
        namespace_filter,
    ) {
        linker.func_wrap("wasi_snapshot_preview1", "path_unlink_file", |_: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::path_unlink_file` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "poll_oneoff", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "poll_oneoff", |_: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::poll_oneoff` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "proc_exit", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "proc_exit", |_: i32| -> Result<(), Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::proc_exit` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "proc_raise", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "proc_raise", |_: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::proc_raise` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "random_get", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "random_get", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::random_get` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "sched_yield", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "sched_yield", || -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::sched_yield` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "sock_recv", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "sock_recv", |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::sock_recv` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "sock_send", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "sock_send", |_: i32, _: i32, _: i32, _: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::sock_send` unavailable in this environment."))
    })?;
    }
    if !namespace_matches_filter("wasi_snapshot_preview1", "sock_shutdown", namespace_filter) {
        linker.func_wrap("wasi_snapshot_preview1", "sock_shutdown", |_: i32, _: i32| -> Result<i32, Trap> {
        Err(Trap::new("Host function `wasi_snapshot_preview1::sock_shutdown` unavailable in this environment."))
    })?;
    }

    Ok(())
}
