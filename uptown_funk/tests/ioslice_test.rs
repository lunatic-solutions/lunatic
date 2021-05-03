use uptown_funk::{host_functions, memory::Memory, HostFunctions};
use wasmtime;

use std::fs::read;
use std::io::IoSliceMut;

mod common;
use common::*;

#[host_functions(namespace = "env")]
impl Empty {
    fn vectored_read(&self, bufs: &mut [IoSliceMut<'_>]) {
        bufs.iter_mut().enumerate().for_each(|(i, buf)| {
            buf.copy_from_slice(&[i as u8; 8][..]);
        });
    }

    fn vectored_write(&self, _bufs: &[std::io::IoSlice<'_>]) {}
}

#[test]
fn wasmtime_ioslice_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/ioslices.wasm")
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
    let test = instance.get_func("test_mut_ioslice").unwrap().call(&[]);

    assert_eq!(test.is_ok(), true);
}
