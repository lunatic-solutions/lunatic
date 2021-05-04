use uptown_funk::{host_functions, memory::Memory, Executor, HostFunctions};
use wasmtime;

use std::fs::read;

mod common;
use common::*;

#[host_functions(namespace = "env")]
impl Empty {
    fn add(&self, a: MyNumber, b: MyNumber) -> i32 {
        a + b
    }
}

struct MyNumber {
    value: i32,
}

impl std::ops::Add<MyNumber> for MyNumber {
    type Output = i32;

    fn add(self, rhs: MyNumber) -> Self::Output {
        self.value + rhs.value
    }
}

impl uptown_funk::FromWasm<&mut Empty> for MyNumber {
    type From = u32;

    fn from(_: &mut Empty, _: &impl Executor, wasm_u32: u32) -> Result<Self, uptown_funk::Trap> {
        Ok(MyNumber {
            value: wasm_u32 as i32,
        })
    }
}

#[test]
fn wasmtime_custom_type_add_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/custom_types.wasm")
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
