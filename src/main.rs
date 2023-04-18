mod mode;

use mode::{cargo_test, execution};

use anyhow::Result;
use regex::Regex;
use std::collections::VecDeque;
use std::{env, path::PathBuf};

// Lunatic versions under 0.13 implied run
// This checks whether the 0.12 behaviour is wanted with a regex
fn is_run_implied() -> bool {
    if std::env::args().count() < 2 {
        return false;
    }

    // lunatic <foo.wasm> -> Implied run
    // lunatic run <foo.wasm> -> Explicit run
    // lunatic fdskl <foo.wasm> -> Not implied run
    let test_re = Regex::new(r"^(--bench|--dir|.+\.wasm)")
        .expect("BUG: Regex error with lunatic::mode::execution::is_run_implied()");

    test_re.is_match(&std::env::args().nth(1).unwrap())
}

#[tokio::main]
async fn main() -> Result<()> {
    if env::var("OTEL_SERVICE_NAME").is_err() {
        env::set_var("OTEL_SERVICE_NAME", "lunatic");
    }

    // Run is implied from lunatic 0.12
    let augmented_args = if is_run_implied() {
        let mut augmented_args: VecDeque<String> = std::env::args().collect();
        augmented_args.insert(1, "run".to_owned());
        Some(augmented_args.into())
    } else {
        None
    };

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

            let skip_positions = match is_run_implied() {
                true => 1,
                false => 2,
            };

            // Check if the 3rd argument is a rust wasm build in the `deps` directory
            // && none of the other arguments indicate a benchmark
            let mut arguments = env::args().skip(skip_positions);
            match arguments.next() {
                Some(wasm_file) => {
                    test_regex.is_match(&wasm_file) && !arguments.any(|arg| arg == "--bench")
                }

                None => false,
            }
        }
        Err(_) => false,
    };

    let result = if cargo_test {
        cargo_test::test(augmented_args).await
    } else {
        execution::execute(augmented_args).await
    };

    opentelemetry::global::shutdown_tracer_provider();

    result
}
