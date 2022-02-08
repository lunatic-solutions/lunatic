use std::fmt::Debug;

use serde::{Deserialize, Serialize};

/// Configuration structure for environments.
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvConfig {
    // Maximum amount of memory that can be used by processes in bytes
    max_memory: usize,
    // Maximum amount of compute expressed in units of 100k instructions.
    max_fuel: Option<u64>,
    allowed_namespaces: Vec<String>,
    preopened_dirs: Vec<String>,
    wasi_args: Option<Vec<String>>,
    wasi_envs: Option<Vec<(String, String)>>,
}

impl Debug for EnvConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("EnvConfig")
            .field("max_memory", &self.max_memory)
            .field("max_fuel", &self.max_fuel)
            .field("allowed_namespaces", &self.allowed_namespaces)
            .field("preopened_dirs", &self.preopened_dirs)
            .field("wasi_args", &self.wasi_args)
            .field("wasi_envs", &self.wasi_envs)
            .finish()
    }
}

impl EnvConfig {
    /// Create a new environment configuration.
    pub fn new(max_memory: usize, max_fuel: Option<u64>) -> Self {
        Self {
            max_memory,
            max_fuel,
            allowed_namespaces: Vec::new(),
            preopened_dirs: Vec::new(),
            wasi_args: None,
            wasi_envs: None,
        }
    }

    pub fn max_memory(&self) -> usize {
        self.max_memory
    }

    pub fn max_fuel(&self) -> Option<u64> {
        self.max_fuel
    }

    pub fn allowed_namespace(&self) -> &[String] {
        &self.allowed_namespaces
    }

    /// Allow a WebAssembly host function namespace to be used with this config.
    pub fn allow_namespace<S: Into<String>>(&mut self, namespace: S) {
        self.allowed_namespaces.push(namespace.into())
    }

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

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            max_memory: 0xA00000000, // = 4 GB in bytes
            max_fuel: None,
            allowed_namespaces: vec![
                String::from("lunatic::"),
                String::from("wasi_snapshot_preview1::"),
            ],
            preopened_dirs: vec![],
            wasi_args: None,
            wasi_envs: None,
        }
    }
}
