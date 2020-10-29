// This test patches all WASM files in the ./patching folder and compares them to the expected output.

#[cfg(test)]
use pretty_assertions::assert_eq;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use lunatic_vm::normalisation::patch;

fn main() {
    let mut tests = Vec::new();
    find_tests("tests/patching".as_ref(), &mut tests);
    run_tests(tests);
}

// Find all .wat files recursively in a given folder.
fn find_tests(path: &Path, tests: &mut Vec<PathBuf>) {
    for f in path.read_dir().unwrap() {
        let f = f.unwrap();
        if f.file_type().unwrap().is_dir() {
            find_tests(&f.path(), tests);
            continue;
        }
        match f.path().extension().and_then(|s| s.to_str()) {
            Some("wat") => {}
            _ => continue,
        }
        tests.push(f.path());
    }
}

fn run_tests(tests: Vec<PathBuf>) {
    for test in tests {
        let test_content = read_to_string(&test).unwrap();
        let input_expected_output: Vec<_> = test_content.split("EXPECTED-RESULT:").collect();
        let input = input_expected_output.first().unwrap();
        let expected_output = input_expected_output.last().unwrap();

        // Run test on one file
        let output_wasm = run_test(input);
        let output_wat = wasmprinter::print_bytes(&output_wasm).unwrap();

        // Normalize expected_output
        let expected_output_wasm = wat::parse_str(expected_output).unwrap();
        let expected_output_wat = wasmprinter::print_bytes(&expected_output_wasm).unwrap();

        let expected_output_multiline: Vec<&str> =
            expected_output_wat.split("\n").into_iter().collect();

        let output_multiline: Vec<&str> = output_wat.split("\n").into_iter().collect();

        assert_eq!(expected_output_multiline, output_multiline);
    }
}

fn run_test(input: &str) -> Vec<u8> {
    let wasm = wat::parse_str(input).unwrap();
    patch(&wasm).unwrap().1
}
