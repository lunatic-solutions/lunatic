use uptown_funk::{Executor, HostFunctions, StateMarker, host_functions, memory::Memory};
#[cfg(feature = "vm-wasmer")]
use wasmer::{self, Exportable};
#[cfg(feature = "vm-wasmtime")]
use wasmtime;

use std::fs::read;

#[derive(Clone)]
struct SimpleExcutor {
    memory: Memory,
}

impl Executor for SimpleExcutor {
    fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

struct Empty {}

impl StateMarker for Empty {}

#[host_functions(namespace = "env")]
impl Empty {
    fn count_a(&self, words: &str) -> i32 {
        words.matches("a").count() as i32
    }

    fn add(&self, a: &str, b: &str, c: &mut [u8]) {
        c[..a.len()].copy_from_slice(a.as_bytes());
        c[a.len()..].copy_from_slice(b.as_bytes());
    }
}

#[cfg(feature = "vm-wasmtime")]
#[test]
fn wasmtime_ref_str_test() {
    let store = wasmtime::Store::default();
    let wasm = read("tests/wasm/ref_str.wasm")
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
    let test_count = instance
        .get_func("test_count")
        .unwrap()
        .get0::<()>()
        .unwrap();
    assert_eq!(test_count().is_ok(), true);

    let test_add = instance.get_func("test_add").unwrap().get0::<()>().unwrap();
    assert_eq!(test_add().is_ok(), true);
}

#[cfg(feature = "vm-wasmer")]
#[test]
fn wasmer_ref_str_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/ref_str.wasm")
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
    let test_count = instance
        .exports
        .get_function("test_count")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test_count.call().is_ok(), true);

    let test_add = instance
        .exports
        .get_function("test_add")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test_add.call().is_ok(), true);
}
