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
    fn trap(&self) -> Traps {
        Traps {}
    }
}

struct Traps {}

impl uptown_funk::ToWasm for Traps {
    type To = u32;
    type State = Empty;

    fn to(_: &mut Self::State, _: &impl Executor, _: Self) -> Result<u32, uptown_funk::Trap> {
        Err(uptown_funk::Trap::new("Execution traped"))
    }
}

#[cfg(feature = "vm-wasmtime")]
#[test]
fn wasmtime_trap_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/trap.wasm")
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
    let test = instance.get_func("test").unwrap().call(&[]);

    match test {
        Ok(_) => assert_eq!("Did trap", "false"),
        Err(trap) => assert!(trap.to_string().contains("Execution traped")),
    };
}

#[cfg(feature = "vm-wasmer")]
#[test]
fn wasmer_trap_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/trap.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmer::Module::new(&store, wasm).unwrap();
    let mut wasmer_linker = uptown_funk::wasmer::WasmerLinker::new();

    let memory_ty = wasmer::MemoryType::new(32, None, false);
    let memory = wasmer::Memory::new(&store, memory_ty).unwrap();
    wasmer_linker.add("env", "memory", memory.to_export());

    let empty = Empty {};
    let instance_state = SimpleExcutor {
        memory: Memory::from(memory),
    };
    empty.add_to_wasmer_linker(instance_state, &mut wasmer_linker, &store);

    let instance = wasmer::Instance::new(&module, &wasmer_linker).unwrap();
    let test = instance
        .exports
        .get_function("test")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    match test.call() {
        Ok(_) => assert_eq!("Did trap", "false"),
        Err(trap) => {
            println!("{:?}", trap.message());
            assert!(trap.message().contains("Execution traped"));
        }
    };
}
