use uptown_funk::{host_functions, memory::Memory, Executor, HostFunctions};
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

impl uptown_funk::ToWasm<&mut Empty> for Traps {
    type To = u32;

    fn to(_: &mut Empty, _: &impl Executor, _: Self) -> Result<u32, uptown_funk::Trap> {
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
    Empty::add_to_linker(empty, instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().call(&[]);

    match test {
        Ok(_) => assert_eq!("Did trap", "false"),
        Err(trap) => assert!(trap.to_string().contains("Execution traped")),
    };
}
