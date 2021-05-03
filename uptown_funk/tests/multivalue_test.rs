use uptown_funk::{host_functions, memory::Memory, HostFunctions};
use wasmtime;

use std::fs::read;

mod common;
use common::*;

#[host_functions(namespace = "env")]
impl Empty {
    fn leet(&self) -> (i32, f32, i64) {
        (1337, 1337.1337, 1337)
    }
}

#[test]
fn wasmtime_mutivalue_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/multivalue.wasm")
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
    let test = instance.get_func("test").unwrap().call(&[]);
    assert_eq!(test.is_ok(), true);
}
