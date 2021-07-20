use anyhow::Result;

use crate::plugin::Plugin;

/// Configuration structure for environments.
pub struct EnvConfig {
    // Maximum amount of memory that can be used by processes
    max_memory: u64,
    // Maximum amount of compute expressed in gallons.
    max_fuel: Option<u64>,
    allowed_namespaces: Vec<String>,
    plugins: Vec<Plugin>,
}

impl EnvConfig {
    /// Create a new environment configuration.
    pub fn new(max_memory: u64, max_fuel: Option<u64>) -> Self {
        Self {
            max_memory,
            max_fuel,
            allowed_namespaces: Vec::new(),
            plugins: Vec::new(),
        }
    }

    pub fn max_memory(&self) -> u64 {
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

    pub fn plugins(&self) -> &Vec<Plugin> {
        &self.plugins
    }

    /// Add module as a plugin that will be used on all processes in the environment.
    ///
    /// Plugins are just regular WebAssembly modules that can define specific hooks inside the
    /// runtime to modify other modules that are dynamically loaded inside the environment or
    /// spawn environment bound processes.
    pub fn add_plugin(&mut self, module: Vec<u8>) -> Result<()> {
        let plugin = Plugin::new(module)?;
        self.plugins.push(plugin);
        Ok(())
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            max_memory: 0x40000000, // 4 Gb
            max_fuel: None,
            allowed_namespaces: vec![
                String::from("lunatic::"),
                String::from("wasi_snapshot_preview1::"),
            ],
            plugins: vec![],
        }
    }
}
