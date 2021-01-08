use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmer::{self, Exportable};
use wasmtime;

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

impl uptown_funk::FromWasmU32<'_> for MyNumber {
    type State = Empty;

    fn from_u32<InstanceState>(
        _: &mut Self::State,
        _: &InstanceState,
        wasm_u32: u32,
    ) -> Result<Self, uptown_funk::Trap> {
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
    let instance_state = InstanceState {};
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();

    assert_eq!(test().is_ok(), true);
}

#[test]
fn wasmer_custom_type_add_test() {
    let store = wasmer::Store::default();
    let wasm = read("tests/wasm/custom_types.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = wasmer::Module::new(&store, wasm).unwrap();
    let mut wasmer_linker = uptown_funk::wasmer::WasmerLinker::new();

    let memory_ty = wasmer::MemoryType::new(32, None, false);
    let memory = wasmer::Memory::new(&store, memory_ty).unwrap();
    wasmer_linker.add("env", "memory", memory.to_export());

    let empty = Empty {};
    let instance_state = InstanceState {};
    empty.add_to_wasmer_linker(instance_state, &mut wasmer_linker, &store);

    let instance = wasmer::Instance::new(&module, &wasmer_linker).unwrap();
    let test = instance
        .exports
        .get_function("test")
        .unwrap()
        .native::<(), ()>()
        .unwrap();

    assert_eq!(test.call().is_ok(), true);
}
