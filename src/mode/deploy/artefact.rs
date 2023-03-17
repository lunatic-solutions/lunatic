use std::{fs::File, path::PathBuf};

pub struct Artefact {
    name: String,
    path: PathBuf,
    file: File,
}

pub struct ArtefactCollection {}

impl ArtefactCollection {
    pub fn find_compiled_binary(
        binary_name: &str,
        target: &str,
        is_release: bool,
    ) -> Option<PathBuf> {
        let manifest_dir = match std::env::var("CARGO_TARGET_DIR") {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => return None,
        };

        let target_dir = manifest_dir.join(target);
        let build_dir = if is_release {
            target_dir.join("release")
        } else {
            target_dir.join("debug")
        };

        let binary_path = build_dir.join(binary_name);

        if binary_path.exists() && binary_path.is_file() {
            Some(binary_path)
        } else {
            None
        }
    }
}
