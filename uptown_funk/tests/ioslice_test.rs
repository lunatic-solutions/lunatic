use uptown_funk::{host_functions, memory::Memory, HostFunctions};
#[cfg(feature = "vm-wasmer")]
use wasmer::{self, Exportable};
#[cfg(feature = "vm-wasmtime")]
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

#[cfg(feature = "vm-wasmtime")]
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
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance
        .get_func("test_mut_ioslice")
        .unwrap()
        .get0::<()>()
        .unwrap();

    assert_eq!(test().is_ok(), true);
}

#[cfg(feature = "vm-wasmer")]
#[test]
fn wasmer_ioslice_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/ioslices.wasm")
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
        .get_function("test_mut_ioslice")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test.call().is_ok(), true);
}
