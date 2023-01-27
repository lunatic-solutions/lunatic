use std::{fmt::Debug, path::Path};

use lunatic_process::config::ProcessConfig;
use lunatic_process_api::ProcessConfigCtx;
use lunatic_wasi_api::LunaticWasiConfigCtx;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct DefaultProcessConfig {
    // Maximum amount of memory that can be used by processes in bytes
    max_memory: usize,
    // Maximum amount of compute expressed in units of 100k instructions.
    max_fuel: Option<u64>,
    // Can this process compile new WebAssembly modules
    can_compile_modules: bool,
    // Can this process create new configurations
    can_create_configs: bool,
    // Can this process spawn sub-processes
    can_spawn_processes: bool,
    // WASI configs
    preopened_dirs: Vec<String>,
    command_line_arguments: Vec<String>,
    environment_variables: Vec<(String, String)>,
}

impl Debug for DefaultProcessConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("EnvConfig")
            .field("max_memory", &self.max_memory)
            .field("max_fuel", &self.max_fuel)
            .field("preopened_dirs", &self.preopened_dirs)
            .field("args", &self.command_line_arguments)
            .field("envs", &self.environment_variables)
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

impl LunaticWasiConfigCtx for DefaultProcessConfig {
    fn add_environment_variable(&mut self, key: String, value: String) {
        self.environment_variables.push((key, value));
    }

    fn add_command_line_argument(&mut self, argument: String) {
        self.command_line_arguments.push(argument);
    }

    fn preopen_dir(&mut self, dir: String) {
        self.preopened_dirs.push(dir);
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

    pub fn set_command_line_arguments(&mut self, args: Vec<String>) {
        self.command_line_arguments = args;
    }

    pub fn command_line_arguments(&self) -> &Vec<String> {
        &self.command_line_arguments
    }

    pub fn set_environment_variables(&mut self, envs: Vec<(String, String)>) {
        self.environment_variables = envs;
    }

    pub fn environment_variables(&self) -> &Vec<(String, String)> {
        &self.environment_variables
    }
}

impl ProcessConfigCtx for DefaultProcessConfig {
    fn can_compile_modules(&self) -> bool {
        self.can_compile_modules
    }

    fn set_can_compile_modules(&mut self, can: bool) {
        self.can_compile_modules = can
    }

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

    fn can_access_fs_location(&self, path: &std::path::Path) -> bool {
        if path.is_relative() {
            return false;
        }
        self.preopened_dirs()
            .iter()
            .filter_map(|dir| match Path::new(dir).canonicalize() {
                Ok(d) => Some(d),
                Err(e) => None,
            })
            .any(|dir| dir.exists() && path.ancestors().any(|ancestor| ancestor.eq(&dir)))
    }
}

impl Default for DefaultProcessConfig {
    fn default() -> Self {
        Self {
            max_memory: u32::MAX as usize, // = 4 GB
            max_fuel: None,
            can_compile_modules: false,
            can_create_configs: false,
            can_spawn_processes: false,
            preopened_dirs: vec![],
            command_line_arguments: vec![],
            environment_variables: vec![],
        }
    }
}
