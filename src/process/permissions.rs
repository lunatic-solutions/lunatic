use std::{env::current_dir, path::Path, path::PathBuf};

/// Permissions held by a process.
/// All filesystem/networking functions will check for permissions first before performing any operations.
#[derive(Clone)]
pub struct ProcessPermissions {
    filesystem: Vec<FilesystemPermission>,
    networking: Vec<NetworkingPermission>,
}

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq)]
/// Used to express permission level
enum Permission {
    None,
    Read,
    Write,
    ReadWrite,
}

impl ProcessPermissions {
    /// Give read/write access to the current folder.
    pub fn current_dir() -> Self {
        let current_path = current_dir().unwrap();
        let current_dir = FilesystemPermission {
            path: current_path,
            permission: Permission::ReadWrite,
        };
        ProcessPermissions {
            filesystem: vec![current_dir],
            networking: vec![],
        }
    }

    /// Returns true if process has `permission` for `path`, otherwise false.
    #[allow(dead_code)]
    fn path_has_permission(&self, path: &Path, permission: Permission) -> bool {
        self.filesystem
            .iter()
            .map(|fsp| fsp.path_permission(path))
            .any(|fs_permission| fs_permission == permission)
    }
}

#[derive(Clone)]
struct FilesystemPermission {
    path: PathBuf,
    permission: Permission,
}

impl FilesystemPermission {
    /// Returns true if `maybe_sub` is a (sub) directory/file of self.path, otherwise false.
    pub fn path_permission(&self, maybe_sub: &Path) -> Permission {
        if self.path.starts_with(maybe_sub) {
            self.permission
        } else {
            Permission::None
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
enum NetworkingPermission {
    Connect,
    Listen,
}
