/*!
Registries allow you to define "well-known" processes in the environment that can be looked up by
name and version.
*/

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use semver::{Version, VersionReq};

use crate::Process;

/// A local (belonging to an Environment) registry of `WasmProcesses`.
///
/// Processed are registered by `name` and `version`. Semver rules are used when looking up
/// processes.
#[derive(Clone, Default)]
pub struct LocalRegistry {
    map: Arc<RwLock<HashMap<String, Vec<RegistryEntry>>>>,
}

impl LocalRegistry {
    /// Create new LocalRegistry
    pub fn new() -> Self {
        LocalRegistry {
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert process into the registry under a specific name and version.
    ///
    /// The version needs to be a correct semver string (e.g "1.2.3-alpha3") or the insertion will
    /// fail. If the exact same version and name exists it will be overwritten.
    pub fn insert(&self, name: String, version: &str, process: Arc<dyn Process>) -> Result<()> {
        let mut writer = self.map.as_ref().write().unwrap();
        let results = writer.entry(name).or_default();
        let version = Version::parse(version)?;
        match results.iter().position(|entry| version.eq(entry.version())) {
            Some(index) => results[index] = RegistryEntry::new(version, process),
            None => results.push(RegistryEntry::new(version, process)),
        }
        Ok(())
    }

    /// Remove process under name & version from registry
    ///
    /// Exact version matching is used for lookup.
    pub fn remove(&self, name: &str, version: &str) -> Result<Option<Arc<dyn Process>>> {
        let mut writer = self.map.as_ref().write().unwrap();
        if let Some(results) = writer.get_mut(name) {
            let version = Version::parse(version)?;
            if let Some(index) = results.iter().position(|entry| version.eq(entry.version())) {
                return Ok(Some(results.remove(index).process()));
            }
        };

        Ok(None)
    }

    /// Returns process under name & version.
    ///
    /// Semver is used for matching.
    pub fn get(&self, name: &str, version_query: &str) -> Result<Option<Arc<dyn Process>>> {
        let reader = self.map.as_ref().read().unwrap();
        if let Some(results) = reader.get(name) {
            let version_query = VersionReq::parse(version_query)?;
            if let Some(entry) = results
                .iter()
                .rev()
                .find(|entry| version_query.matches(entry.version()))
            {
                return Ok(Some(entry.process()));
            }
        };

        Ok(None)
    }
}

struct RegistryEntry {
    version: Version,
    process: Arc<dyn Process>,
}

impl RegistryEntry {
    fn new(version: Version, process: Arc<dyn Process>) -> Self {
        Self { version, process }
    }

    fn process(&self) -> Arc<dyn Process> {
        self.process.clone()
    }

    fn version(&self) -> &Version {
        &self.version
    }
}

#[cfg(test)]
mod tests {
    use super::LocalRegistry;
    use crate::{Process, Signal};
    use std::sync::Arc;
    use uuid::Uuid;

    #[derive(Clone, Debug)]
    struct IdentityProcess(Uuid);
    impl Process for IdentityProcess {
        fn id(&self) -> Uuid {
            self.0
        }
        fn send(&self, _: Signal) {}
    }

    #[test]
    fn registry_test() {
        let registry = LocalRegistry::new();
        let proc = Arc::new(IdentityProcess(Uuid::new_v4()));
        // Inserting an incorrect version fails
        let result = registry.insert("test".to_string(), "", proc.clone());
        assert!(result.is_err());
        // Insert version 0.0.0
        let result = registry.insert("test".to_string(), "0.0.0", proc.clone());
        assert!(result.is_ok());
        // Empty query should fail
        let result = registry.get("test", "");
        assert!(result.is_err());
        // Wildcard should match any version
        let result = registry.get("test", "*").unwrap().unwrap();
        assert_eq!(result.id(), proc.id());
        // Removing 0.0.0 should return the correct process
        let result = registry.remove("test", "0.0.0").unwrap().unwrap();
        assert_eq!(result.id(), proc.id());

        // Insert version 1.1.0
        let proc1 = Arc::new(IdentityProcess(Uuid::new_v4()));
        let result = registry.insert("test".to_string(), "1.1.0", proc1.clone());
        assert!(result.is_ok());
        // Insert version 1.2.0
        let proc2 = Arc::new(IdentityProcess(Uuid::new_v4()));
        let result = registry.insert("test".to_string(), "1.2.0", proc2.clone());
        assert!(result.is_ok());
        // Looking up ^1 should return the latest insert
        let result = registry.get("test", "^1").unwrap().unwrap();
        assert_eq!(result.id(), proc2.id());
        // Removing 1.2.0 should remove proc2
        let result = registry.remove("test", "1.2.0").unwrap().unwrap();
        assert_eq!(result.id(), proc2.id());
        // Looking up ^1 again should return the only left match
        let result = registry.get("test", "^1").unwrap().unwrap();
        assert_eq!(result.id(), proc1.id());
    }
}
