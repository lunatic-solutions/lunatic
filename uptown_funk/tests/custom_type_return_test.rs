use uptown_funk::{host_functions, memory::Memory, Executor, HostFunctions};
#[cfg(feature = "vm-wasmer")]
use wasmer::{self, Exportable};
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

impl uptown_funk::ToWasm for MyNumber {
    type To = u32;
    type State = Empty;

    fn to(_: &mut Self::State, _: &impl Executor, number: Self) -> Result<u32, uptown_funk::Trap> {
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
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();

    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();
    assert_eq!(test().is_ok(), true);

    let test_mutlivalue = instance
        .get_func("test_multivalue")
        .unwrap()
        .get0::<()>()
        .unwrap();
    assert_eq!(test_mutlivalue().is_ok(), true);
}

#[cfg(feature = "vm-wasmer")]
#[test]
fn wasmer_custom_type_return_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/custom_types_return.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmer::Module::new(&store, wasm).unwrap();
    let mut wasmer_linker = uptown_funk::wasmer::WasmerLinker::new();

    let memory_ty = wasmer::MemoryType::new(32, None, false);
    let memory = wasmer::Memory::new(&store, memory_ty).unwrap();
    wasmer_linker.add("env", "memory", memory.to_export());

    let empty = Empty {};
    let instance_state = SimpleExcutor {
        memory: Memory::Wasmer(memory),
    };
    empty.add_to_wasmer_linker(instance_state, &mut wasmer_linker, &store);

    let instance = wasmer::Instance::new(&module, &wasmer_linker).unwrap();
    let test = instance
        .exports
        .get_function("test")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test.call().is_ok(), true);

    let test_multivalue = instance
        .exports
        .get_function("test_multivalue")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test_multivalue.call().is_ok(), true);
}
