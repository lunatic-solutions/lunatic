use std::{fs::File, io::Read, path::PathBuf};

pub(crate) static TARGET_DIR: &str = "./lunatic_target";

pub fn get_target_dir() -> PathBuf {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    current_dir.join(TARGET_DIR)
}

///
pub fn find_compiled_binary(binary_name: String, target: &str, is_release: bool) -> Vec<u8> {
    let target_dir = get_target_dir().join(target);
    let build_dir = if is_release {
        target_dir.join("release")
    } else {
        target_dir.join("debug")
    };

    let binary_path = build_dir.join(binary_name);

    if binary_path.exists() && binary_path.is_file() {
        let mut wasm_file = File::open(binary_path).expect("Failed to open file '{binary_path}'");
        let mut buf = vec![];
        wasm_file
            .read_to_end(&mut buf)
            .expect("Failed to read wasm file '{binary_path}'");
        buf
    } else {
        panic!("Failed to find wasm file at '{:?}'", binary_path);
    }
}
