use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

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

impl uptown_funk::FromWasmI32 for MyNumber {
    type State = Empty;

    fn from_i32<InstanceState>(
        _state: &Self::State,
        _instance_environment: &InstanceState,
        wasm_i32: i32,
    ) -> Result<Self, uptown_funk::Trap> {
        Ok(MyNumber { value: wasm_i32 })
    }
}

#[test]
fn custom_type_add_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/custom_types.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = Module::new(store.engine(), wasm).unwrap();
    let mut linker = Linker::new(&store);

    let empty = Empty {};
    let instance_state = InstanceState {};
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();

    assert_eq!(test().is_ok(), true);
}
