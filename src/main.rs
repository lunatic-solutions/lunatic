mod mode;

use mode::{cargo_test, execution};

use anyhow::Result;
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    // Detect if `cargo test` is running
    // https://internals.rust-lang.org/t/cargo-config-tom-different-runner-for-tests/16342/
    let cargo_test = match env::var("CARGO_MANIFEST_DIR") {
        Ok(_manifest_dir) => {
            // _manifest_dir is not used as a prefix because it breaks testing in workspaces where
            // the `target` dir lives outside the manifest dir.
            let test_path_matcher: PathBuf = [
                "target",
                "wasm32-(wasi|unknown-unknown)",
                "(debug|release)",
                "deps",
            ]
            .iter()
            .collect();
            // Escape \ if it is used as path separator
            let separator = format!("{}", std::path::MAIN_SEPARATOR).replace('\\', r"\\");
            let test_path_matcher = test_path_matcher.to_string_lossy().replace('\\', r"\\");
            // Regex that will match test builds
            let test_regex = format!("{separator}{test_path_matcher}{separator}.*\\.wasm$");
            let test_regex = regex::Regex::new(&test_regex).unwrap();

            let mut arguments = env::args().skip(1);
            match arguments.next() {
                Some(wasm_file) => {
                    // Check if the second argument is a rust wasm build in the `deps` directory
                    // && none of the other arguments indicate a benchmark
                    test_regex.is_match(&wasm_file) && !arguments.any(|arg| arg == "--bench")
                }
                None => false,
            }
        }
        Err(_) => false,
    };

    if cargo_test {
        cargo_test::test().await
    } else {
        execution::execute().await
    }
}
