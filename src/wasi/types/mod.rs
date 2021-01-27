#![allow(dead_code)]

mod aliases;
mod flags;
mod status;
mod structs;

pub use aliases::*;
pub use flags::*;
pub use status::Status;
pub use status::StatusResult;
pub use status::StatusTrap;
pub use status::StatusTrapResult;
pub use structs::*;

use uptown_funk::{Executor, FromWasm, Trap};
// TODO remove dependency
use wasi_common::wasi::types::Clockid;

pub struct Wrap<T> {
    pub inner: T,
}

impl<T> Wrap<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl FromWasm for Wrap<Clockid> {
    type From = u32;
    type State = ();

    fn from(
        _state: &mut Self::State,
        _executor: &impl Executor,
        from: Self::From,
    ) -> Result<Self, Trap>
    where
        Self: Sized,
    {
        use std::convert::TryFrom;
        Clockid::try_from(from)
            .map_err(|_| Trap::new("Invalid clock id"))
            .map(|v| Wrap::new(v))
    }
}

pub struct WasiEnv {
    bytes: Vec<Vec<u8>>,
    total_bytes: u32,
}

impl WasiEnv {
    pub fn env_vars(vars: impl Iterator<Item = (String, String)>) -> Self {
        let mut bytes = vec![];
        for (k, v) in vars {
            bytes.push(format!("{}={}\0", k, v).into_bytes());
        }

        let total_bytes = bytes.iter().map(|v| v.len() as u32).sum();

        Self { bytes, total_bytes }
    }

    pub fn args(vars: impl Iterator<Item = String>) -> Self {
        let mut bytes = vec![];
        for v in vars {
            bytes.push(format!("{}\0", v).into_bytes());
        }

        let total_bytes = bytes.iter().map(|v| v.len() as u32).sum();

        Self { bytes, total_bytes }
    }

    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub fn total_bytes(&self) -> u32 {
        self.total_bytes
    }

    pub fn iter(&self) -> std::slice::Iter<Vec<u8>> {
        self.bytes.iter()
    }
}

pub struct ExitCode {}

impl FromWasm for ExitCode {
    type From = u32;
    type State = ();

    fn from(
        _: &mut Self::State,
        _: &impl Executor,
        exit_code: u32,
    ) -> Result<Self, uptown_funk::Trap> {
        Err(uptown_funk::Trap::new(format!(
            "proc_exit({}) called",
            exit_code
        )))
    }
}

pub const WASI_STDIN_FILENO: u32 = 0;
pub const WASI_STDOUT_FILENO: u32 = 1;
pub const WASI_STDERR_FILENO: u32 = 2;
