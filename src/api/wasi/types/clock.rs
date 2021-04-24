use uptown_funk::{types::CReprWasmType, Executor, FromWasm, Trap};

#[derive(Copy, Clone)]
#[repr(u32)]
pub enum Clockid {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTimeId = 2,
    ThreadCpuTimeId = 3,
    Unsupported = u32::MAX,
}

fn to_clockid(num: u32) -> Clockid {
    match num {
        0 => Clockid::Realtime,
        1 => Clockid::Monotonic,
        2 => Clockid::ProcessCpuTimeId,
        3 => Clockid::ThreadCpuTimeId,
        _ => Clockid::Unsupported,
    }
}

impl CReprWasmType for Clockid {}

impl<S> FromWasm<S> for Clockid {
    type From = u32;

    fn from(_: S, _: &impl Executor, from: u32) -> Result<Self, Trap> {
        Ok(to_clockid(from))
    }
}
