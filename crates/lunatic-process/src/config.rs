use serde::{de::DeserializeOwned, Serialize};

// One unit of fuel represents around 100k instructions.
pub const UNIT_OF_COMPUTE_IN_INSTRUCTIONS: u64 = 100_000;

/// Common process configuration.
///
/// Each process in lunatic can have specific limits and permissions. These properties are set
/// through a process configuration that is used when a process is spawned. Once the process is
/// spawned the configuration can't be changed anymore. The process configuration heavily depends
/// on the [`ProcessState`](crate::state::ProcessState) that defines host functions available to
/// the process. This host functions are the ones that consider specific configuration while
/// performing operations.
///
/// However, two properties of a process are enforced by the runtime (maximum memory and maximum
/// fuel usage). This two properties need to be part of every configuration.
///
/// `ProcessConfig` must be serializable in case it is used to spawn processes on other nodes.
pub trait ProcessConfig: Clone + Serialize + DeserializeOwned {
    fn set_max_fuel(&mut self, max_fuel: Option<u64>);
    fn get_max_fuel(&self) -> Option<u64>;
    fn set_max_memory(&mut self, max_memory: usize);
    fn get_max_memory(&self) -> usize;
}
