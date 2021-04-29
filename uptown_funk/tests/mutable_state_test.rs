use uptown_funk::{host_functions, memory::Memory, Executor, HostFunctions};
#[cfg(feature = "vm-wasmer")]
use wasmer::{self, Exportable};
#[cfg(feature = "vm-wasmtime")]
use wasmtime;

use std::fs::read;

mod common;
use common::*;

struct ArrayState {
    vec: Vec<MyNumber>,
}

#[host_functions(namespace = "env")]
impl ArrayState {
    fn create(&mut self, number: i32) -> MyNumber {
        MyNumber::new(number)
    }

    fn value(&self, number: MyNumber) -> i32 {
        number.value
    }

    fn add(&mut self, a: MyNumber, b: MyNumber) -> MyNumber {
        a + b
    }

    fn sum(&self) -> i32 {
        self.vec.iter().map(|n| n.value).sum()
    }
}

#[derive(Clone)]
struct MyNumber {
    value: i32,
}

impl MyNumber {
    fn new(value: i32) -> Self {
        Self { value }
    }
}

impl std::ops::Add<MyNumber> for MyNumber {
    type Output = MyNumber;

    fn add(self, rhs: MyNumber) -> Self::Output {
        MyNumber {
            value: self.value + rhs.value,
        }
    }
}

impl uptown_funk::FromWasm<&mut ArrayState> for MyNumber {
    type From = u32;

    fn from(
        state: &mut ArrayState,
        _: &impl Executor,
        index: u32,
    ) -> Result<Self, uptown_funk::Trap> {
        match state.vec.get(index as usize) {
            Some(number) => Ok(number.clone()),
            None => Err(uptown_funk::Trap::new("Number not found")),
        }
    }
}

impl uptown_funk::ToWasm<&mut ArrayState> for MyNumber {
    type To = u32;

    fn to(
        state: &mut ArrayState,
        _: &impl Executor,
        number: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        let index = state.vec.len();
        state.vec.push(number);
        Ok(index as u32)
    }
}

#[cfg(feature = "vm-wasmtime")]
#[test]
fn wasmtime_mutable_state_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/mutable_state.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmtime::Module::new(store.engine(), wasm).unwrap();
    let mut linker = wasmtime::Linker::new(&store);

    let memory_ty = wasmtime::MemoryType::new(wasmtime::Limits::new(32, None));
    let memory = wasmtime::Memory::new(&store, memory_ty);
    linker.define("env", "memory", memory.clone()).unwrap();

    let array_state = ArrayState { vec: Vec::new() };
    let instance_state = SimpleExcutor {
        memory: Memory::from(memory),
    };
    ArrayState::add_to_linker(array_state, instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().call(&[]);

    assert_eq!(test.is_ok(), true);
}

#[cfg(feature = "vm-wasmer")]
#[test]
fn wasmer_mutable_state_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/mutable_state.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmer::Module::new(&store, wasm).unwrap();
    let mut wasmer_linker = uptown_funk::wasmer::WasmerLinker::new();

    let memory_ty = wasmer::MemoryType::new(32, None, false);
    let memory = wasmer::Memory::new(&store, memory_ty).unwrap();
    wasmer_linker.add("env", "memory", memory.to_export());

    let array_state = ArrayState { vec: Vec::new() };
    let instance_state = SimpleExcutor {
        memory: Memory::from(memory),
    };
    ArrayState::add_to_wasmer_linker(array_state, instance_state, &mut wasmer_linker, &store);

    let instance = wasmer::Instance::new(&module, &wasmer_linker).unwrap();
    let test = instance
        .exports
        .get_function("test")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test.call().is_ok(), true);
}
