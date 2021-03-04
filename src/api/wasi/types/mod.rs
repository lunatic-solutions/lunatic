#![allow(dead_code)]

mod aliases;
mod clock;
mod flags;
mod status;
mod structs;

pub use aliases::*;
pub use clock::*;
pub use flags::*;
pub use status::Status;
pub use status::StatusResult;
pub use status::StatusTrap;
pub use status::StatusTrapResult;
pub use structs::*;

pub struct Wrap<T> {
    pub inner: T,
}

impl<T> Wrap<T> {
    fn new(inner: T) -> Self {
        Self { inner }
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
