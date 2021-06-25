use std::fs;
use wat::parse_file;

// This script is used to generate .wasm files from .wat files for benchmarks and tests.
//
// It will write all generated .wasm files into the `./target/wasm` directory.
// TODO: This should only run before `cargo test` or `cargo bench`, but this is currently not
//       possible to detect from build scripts (https://github.com/rust-lang/cargo/issues/4001).
fn main() {
    const WAT_DIR: &str = "wat";
    const TARGET_DIR: &str = "target/wasm/";

    // Re-run if any file in the `wat` directory changes
    println!("cargo:rerun-if-changed={}", WAT_DIR);

    // Create output directory if it doesn't exist
    fs::create_dir_all(TARGET_DIR).expect(&format!("Create {} dir", TARGET_DIR));

    // Scan `wat` directory for .wat files and build corresponding .wasm files
    for wat_file in fs::read_dir(WAT_DIR).expect(&format!("Read {}", WAT_DIR)) {
        let wat_file = wat_file.unwrap();
        let wasm = parse_file(wat_file.path()).expect(&format!("Parsing {:?}", wat_file.path()));
        // Change extension to .wasm
        let wasm_filename = wat_file.path().with_extension("wasm");
        // Get only the filename part of the `Path`
        let wasm_filename = wasm_filename.file_name().unwrap().to_str().unwrap();
        let wasm_file = format!("{}{}", TARGET_DIR, wasm_filename);
        fs::write(&wasm_file, wasm).expect(&format!("Writing {}", wasm_file));
    }
}
