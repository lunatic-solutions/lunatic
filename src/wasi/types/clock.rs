use uptown_funk::{types::CReprWasmType, Executor, FromWasm, Trap};

#[derive(Copy, Clone)]
#[repr(u32)]
pub enum Clockid {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTimeId = 2,
    ThreadCpuTimeId = 3,
}

impl CReprWasmType for Clockid {}

impl FromWasm for Clockid {
    type From = u32;
    type State = ();

    fn from(_: &mut (), _: &impl Executor, from: u32) -> Result<Self, Trap> {
        match from {
            0 => Ok(Clockid::Realtime),
            1 => Ok(Clockid::Monotonic),
            2 => Ok(Clockid::ProcessCpuTimeId),
            3 => Ok(Clockid::ThreadCpuTimeId),
            // FIXME: can I throw Status::Inval here?
            _ => Err(Trap::new("Invalid clockid")),
        }
    }
}
