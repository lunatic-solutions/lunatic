use std::fmt::Debug;

use lunatic_process::config::ProcessConfig;
use lunatic_process_api::ProcessConfigCtx;
use serde::{Deserialize, Serialize};

///
#[derive(Clone, Serialize, Deserialize)]
pub struct DefaultProcessConfig {
    // Maximum amount of memory that can be used by processes in bytes
    max_memory: usize,
    // Maximum amount of compute expressed in units of 100k instructions.
    max_fuel: Option<u64>,
    // Can this process create new configurations
    can_create_configs: bool,
    // Can this process spawn sub-processes
    can_spawn_processes: bool,
    preopened_dirs: Vec<String>,
    wasi_args: Option<Vec<String>>,
    wasi_envs: Option<Vec<(String, String)>>,
}

impl Debug for DefaultProcessConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("EnvConfig")
            .field("max_memory", &self.max_memory)
            .field("max_fuel", &self.max_fuel)
            .field("preopened_dirs", &self.preopened_dirs)
            .field("wasi_args", &self.wasi_args)
            .field("wasi_envs", &self.wasi_envs)
            .finish()
    }
}

impl ProcessConfig for DefaultProcessConfig {
    fn set_max_fuel(&mut self, max_fuel: Option<u64>) {
        self.max_fuel = max_fuel;
    }

    fn get_max_fuel(&self) -> Option<u64> {
        self.max_fuel
    }

    fn set_max_memory(&mut self, max_memory: usize) {
        self.max_memory = max_memory
    }

    fn get_max_memory(&self) -> usize {
        self.max_memory
    }
}

impl DefaultProcessConfig {
    pub fn preopened_dirs(&self) -> &[String] {
        &self.preopened_dirs
    }

    /// Grant access to the given directory with this config.
    pub fn preopen_dir<S: Into<String>>(&mut self, dir: S) {
        self.preopened_dirs.push(dir.into())
    }

    pub fn set_wasi_args(&mut self, args: Vec<String>) {
        self.wasi_args = Some(args);
    }

    pub fn wasi_args(&self) -> &Option<Vec<String>> {
        &self.wasi_args
    }

    pub fn set_wasi_envs(&mut self, envs: Vec<(String, String)>) {
        self.wasi_envs = Some(envs);
    }

    pub fn wasi_envs(&self) -> &Option<Vec<(String, String)>> {
        &self.wasi_envs
    }
}

impl ProcessConfigCtx for DefaultProcessConfig {
    fn can_create_configs(&self) -> bool {
        self.can_create_configs
    }

    fn set_can_create_configs(&mut self, can: bool) {
        self.can_create_configs = can
    }

    fn can_spawn_processes(&self) -> bool {
        self.can_spawn_processes
    }

    fn set_can_spawn_processes(&mut self, can: bool) {
        self.can_spawn_processes = can
    }
}

impl Default for DefaultProcessConfig {
    fn default() -> Self {
        Self {
            max_memory: u32::MAX as usize, // = 4 GB
            max_fuel: None,
            can_create_configs: false,
            can_spawn_processes: false,
            preopened_dirs: vec![],
            wasi_args: None,
            wasi_envs: None,
        }
    }
}
