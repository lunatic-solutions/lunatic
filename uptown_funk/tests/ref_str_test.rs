use uptown_funk::{host_functions, memory::Memory, HostFunctions};
use wasmtime;

use std::fs::read;

mod common;
use common::*;

#[host_functions(namespace = "env")]
impl Empty {
    fn count_a(&self, words: &str) -> i32 {
        words.matches("a").count() as i32
    }

    fn add(&self, a: &str, b: &str, c: &mut [u8]) {
        c[..a.len()].copy_from_slice(a.as_bytes());
        c[a.len()..].copy_from_slice(b.as_bytes());
    }
}

#[test]
fn wasmtime_ref_str_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/ref_str.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmtime::Module::new(store.engine(), wasm).unwrap();
    let mut linker = wasmtime::Linker::new(&store);

    let memory_ty = wasmtime::MemoryType::new(wasmtime::Limits::new(32, None));
    let memory = wasmtime::Memory::new(&store, memory_ty);
    linker.define("env", "memory", memory.clone()).unwrap();

    let empty = Empty {};
    let instance_state = SimpleExcutor {
        memory: Memory::from(memory),
    };
    Empty::add_to_linker(empty, instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test_count = instance.get_func("test_count").unwrap().call(&[]);
    assert_eq!(test_count.is_ok(), true);

    let test_add = instance.get_func("test_add").unwrap().call(&[]);
    assert_eq!(test_add.is_ok(), true);
}
