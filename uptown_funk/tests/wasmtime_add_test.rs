use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

use std::fs::read;

struct InstanceState {}

impl InstanceEnvironment for InstanceState {
    fn wasm_memory(&self) -> &mut [u8] {
        &mut []
    }
}

struct Empty {}

#[host_functions(namespace = "env")]
impl Empty {
    fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

#[test]
fn add_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/add.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = Module::new(store.engine(), wasm).unwrap();
    let mut linker = Linker::new(&store);

    let empty = Empty {};
    let instance_state = InstanceState {};
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();

    assert_eq!(test().is_ok(), true);
}
