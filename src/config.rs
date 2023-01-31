use std::{
    fmt::Debug,
    path::{Component, Path, PathBuf},
};

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

    fn can_access_fs_location(&self, path: &std::path::Path) -> Result<(), String> {
        let (file_path, parent_dir) = match strip_file(path) {
            Ok(p) => p,
            Err(e) => {
                return Err(e.to_string());
            }
        };
        let has_access = self
            .preopened_dirs()
            .iter()
            .filter_map(|dir| match get_absolute_path(Path::new(dir)) {
                Ok(d) => Some(d),
                _ => None,
            })
            .any(|dir| dir.exists() && path_is_ancestor(&dir, &parent_dir));

        match has_access {
            true => Ok(()),
            false => Err(format!("Permission to '{file_path:?}' denied")),
        }
    }
}

fn path_is_ancestor(ancestor: &Path, descendant: &Path) -> bool {
    let ancestor_path = Path::new(ancestor);
    let descendant_path = Path::new(descendant);

    if !ancestor_path.is_dir() {
        return false;
    }

    // If the ancestor path is root, return true
    if ancestor_path.as_os_str() == Path::new("/").as_os_str() {
        return true;
    }

    let descendant_components = descendant_path.ancestors();

    // Check if each component of the descendant path starts with the ancestor path
    for component in descendant_components {
        if component.as_os_str() == ancestor_path.as_os_str() {
            return true;
        }
    }

    false
}

// returns a tuple of paths, where the first is the full resolved canonicalized path
// and the second one is stripped of the file extension, pointing to the parent directory
// of the file that a program is trying to access
fn strip_file(path: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let absolute_path = get_absolute_path(path)?;
    if absolute_path.is_file() {
        return Ok((absolute_path.clone(), absolute_path.join("..")));
    }
    Ok((absolute_path.clone(), absolute_path))
}

fn get_absolute_path(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let path = if path.is_relative() {
        Path::join(std::env::current_dir().unwrap().as_path(), path)
    } else {
        path.to_path_buf()
    };
    Ok(normalize_path(&path))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::{get_absolute_path, path_is_ancestor};

    use super::normalize_path;

    #[test]
    fn test_accessible_paths() {
        let crates = get_absolute_path(Path::new("crates")).unwrap();
        let sqlite = get_absolute_path(Path::new("crates/lunatic-sqlite-api")).unwrap();
        let src = get_absolute_path(Path::new("crates/lunatic-sqlite-api/src")).unwrap();
        let guest_api =
            get_absolute_path(Path::new("crates/lunatic-sqlite-api/src/guest_api")).unwrap();
        // checks
        assert!(path_is_ancestor(&crates, &guest_api));
        assert!(path_is_ancestor(&sqlite, &guest_api));
        assert!(path_is_ancestor(&src, &guest_api));
        assert!(path_is_ancestor(&guest_api, &guest_api));
    }

    #[test]
    fn test_forbidden_paths() {
        let crates = get_absolute_path(Path::new("crates")).unwrap();
        let sqlite = get_absolute_path(Path::new("crates/lunatic-sqlite-api")).unwrap();
        let src = get_absolute_path(Path::new("crates/lunatic-sqlite-api/src")).unwrap();
        let guest_api =
            get_absolute_path(Path::new("crates/lunatic-sqlite-api/src/guest_api")).unwrap();
        // checks that there's no access to any ancestor paths
        assert_eq!(path_is_ancestor(&guest_api, &crates), false);
        assert_eq!(path_is_ancestor(&guest_api, &sqlite), false);
        assert_eq!(path_is_ancestor(&guest_api, &src), false);
    }

    #[test]
    fn test_forbidden_absolute_paths() {
        let src = get_absolute_path(Path::new("crates/lunatic-sqlite-api/src")).unwrap();
        // checks that there's no access to any ancestor paths
        assert_eq!(path_is_ancestor(&src, Path::new("/")), false);
        assert_eq!(path_is_ancestor(&src, Path::new("/etc/passwd")), false);
    }

    #[test]
    fn normalized_paths() {
        let crates = get_absolute_path(Path::new("crates")).unwrap();
        let src = get_absolute_path(Path::new("crates/lunatic-sqlite-api/src")).unwrap();
        let sneaky_src =
            get_absolute_path(Path::new("crates/lunatic-sqlite-api/src/../src/.")).unwrap();
        let sneaky_path =
            get_absolute_path(Path::new("crates/lunatic-sqlite-api/src/../src/../../")).unwrap();
        assert_eq!(src, normalize_path(&sneaky_src));
        assert_eq!(crates, normalize_path(&sneaky_path));
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
