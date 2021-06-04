// NOTE: implementation borrowed from https://github.com/wasmerio/wasmer/blob/0ab8a0de096ffdf89f353dd722a15b5e6255055f/lib/wasi/src/syscalls/unix/mod.rs

use super::types::*;
use libc::{
    clock_getres, clock_gettime, timespec, CLOCK_MONOTONIC, CLOCK_PROCESS_CPUTIME_ID,
    CLOCK_REALTIME, CLOCK_THREAD_CPUTIME_ID,
};
use std::{os::unix::fs::symlink, path::Path};

use uptown_funk::types::Pointer;

fn errno_to_status(err: i32) -> Status {
    // TODO: map errno from clock_getres to types::Status
    match err {
        0 => Status::Success,
        _ => Status::Inval,
    }
}

pub fn platform_clock_res_get(clock_id: Clockid, mut res: Pointer<Timestamp>) -> Status {
    let unix_clock_id = match clock_id {
        Clockid::Realtime => CLOCK_REALTIME,
        Clockid::Monotonic => CLOCK_MONOTONIC,
        Clockid::ProcessCpuTimeId => CLOCK_PROCESS_CPUTIME_ID,
        Clockid::ThreadCpuTimeId => CLOCK_THREAD_CPUTIME_ID,
        Clockid::Unsupported => return Status::Inval,
    };

    let (output, timespec_out) = unsafe {
        let mut timespec_out: timespec = timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        (clock_getres(unix_clock_id, &mut timespec_out), timespec_out)
    };

    let t_out = (timespec_out.tv_sec * 1_000_000_000).wrapping_add(timespec_out.tv_nsec);
    res.set(t_out as Timestamp);

    errno_to_status(output)
}

pub fn platform_clock_time_get(
    clock_id: Clockid,
    _precision: Timestamp,
    mut time: Pointer<Timestamp>,
) -> StatusTrapResult {
    let unix_clock_id = match clock_id {
        Clockid::Realtime => CLOCK_REALTIME,
        Clockid::Monotonic => CLOCK_MONOTONIC,
        Clockid::ProcessCpuTimeId => CLOCK_PROCESS_CPUTIME_ID,
        Clockid::ThreadCpuTimeId => CLOCK_THREAD_CPUTIME_ID,
        Clockid::Unsupported => return Status::Inval.into(),
    };

    let (output, timespec_out) = unsafe {
        let mut timespec_out: timespec = timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        (
            clock_gettime(unix_clock_id, &mut timespec_out),
            timespec_out,
        )
    };

    let t_out = (timespec_out.tv_sec * 1_000_000_000).wrapping_add(timespec_out.tv_nsec);
    time.set(t_out as Timestamp);

    errno_to_status(output).into()
}

pub fn platform_symlink<P: AsRef<Path>>(old_path: P, new_path: P) -> StatusResult {
    symlink(old_path, new_path)?;
    Status::Success.into()
}
