use uptown_funk::{host_functions, memory::Memory, Executor, HostFunctions};
#[cfg(feature = "vm-wasmtime")]
use wasmtime;

use std::fs::read;

mod common;
use common::*;

#[host_functions(namespace = "env")]
impl Empty {
    fn return_7(&self) -> MyNumber {
        MyNumber { value: 7 }
    }

    fn return_1_2_3(&self) -> (MyNumber, MyNumber, MyNumber) {
        (
            MyNumber { value: 1 },
            MyNumber { value: 2 },
            MyNumber { value: 3 },
        )
    }
}

struct MyNumber {
    value: i32,
}

impl uptown_funk::ToWasm<&mut Empty> for MyNumber {
    type To = u32;

    fn to(_: &mut Empty, _: &impl Executor, number: Self) -> Result<u32, uptown_funk::Trap> {
        Ok(number.value as u32)
    }
}

#[cfg(feature = "vm-wasmtime")]
#[test]
fn wasmtime_custom_type_return_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/custom_types_return.wasm")
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

    let test_mutlivalue = instance.get_func("test_multivalue").unwrap().call(&[]);
    assert_eq!(test_mutlivalue.is_ok(), true);
}
