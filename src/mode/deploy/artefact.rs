use std::path::PathBuf;

use crate::mode::config::{FileBased, ProjectLunaticConfig};

pub(crate) static TARGET_DIR: &str = "target";

pub fn get_target_dir() -> PathBuf {
    let mut current_dir =
        ProjectLunaticConfig::get_file_path().expect("should have found config path");
    current_dir.pop();
    current_dir.join(TARGET_DIR)
}
