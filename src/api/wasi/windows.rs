// NOTE: implementation borrowed from https://github.com/wasmerio/wasmer/blob/0ab8a0de096ffdf89f353dd722a15b5e6255055f/lib/wasi/src/syscalls/windows.rs

use super::types::*;
use std::time::{SystemTime, UNIX_EPOCH};
use winapi::um::sysinfoapi::GetTickCount64;

use uptown_funk::{types::Pointer, Trap};

pub fn platform_clock_res_get(clock_id: Clockid, mut res: Pointer<Timestamp>) -> Status {
    let resolution_val = match clock_id {
        // resolution of monotonic clock at 10ms, from:
        // https://docs.microsoft.com/en-us/windows/desktop/api/sysinfoapi/nf-sysinfoapi-gettickcount64
        Clockid::Realtime => 1,
        Clockid::Monotonic => 10_000_000,
        // TODO: verify or compute this
        Clockid::ProcessCpuTimeId => {
            return Status::Inval;
        }
        Clockid::ThreadCpuTimeId => {
            return Status::Inval;
        }
        Clockid::Unsupported => return Status::Inval,
    };
    res.set(resolution_val);
    Status::Success
}

pub fn platform_clock_time_get(
    clock_id: Clockid,
    _precision: Timestamp,
    mut time: Pointer<Timestamp>,
) -> StatusTrapResult {
    let nanos =
        match clock_id {
            Clockid::Realtime => {
                let duration = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| Status::Io)?;
                duration.as_nanos() as u64
            }
            Clockid::Monotonic => {
                let tick_ms = unsafe { GetTickCount64() };
                tick_ms * 1_000_000
            }

            Clockid::ProcessCpuTimeId => return Err(Trap::new(
                "wasi::api::platform_clock_time_get(Clockid::ProcessCpuTimeId, ..) not implemented",
            )
            .into()),
            Clockid::ThreadCpuTimeId => return Err(Trap::new(
                "wasi::api::platform_clock_time_get(Clockid::ThreadCpuTimeId, ..) not implemented",
            )
            .into()),
            Clockid::Unsupported => return Status::Inval.into(),
        };
    time.set(nanos);
    Ok(())
}
