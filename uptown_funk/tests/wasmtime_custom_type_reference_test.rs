use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

use std::fs::read;

struct InstanceState {}

impl InstanceEnvironment for InstanceState {
    fn wasm_memory(&self) -> &mut [u8] {
        &mut []
    }
}

struct NumberState {
    saved_number: MyNumber,
}

#[host_functions(namespace = "env")]
impl NumberState {
    fn return_1337(&self, a: &MyNumber) -> MyNumber {
        a.clone()
    }
}

#[derive(Clone)]
struct MyNumber {
    value: i32,
}

impl uptown_funk::FromWasmI32Borrowed for MyNumber {
    type State = NumberState;

    fn from_i32_borrowed<'a, I>(
        state: &'a Self::State,
        _instance_environment: &I,
        _wasm_i32: i32,
    ) -> Result<&'a Self, uptown_funk::Trap>
    where
        I: InstanceEnvironment,
    {
        Ok(&state.saved_number)
    }
}

impl uptown_funk::ToWasmI32 for MyNumber {
    type State = NumberState;

    fn to_i32<InstanceState>(
        _state: &Self::State,
        _instance_environment: &InstanceState,
        number: Self,
    ) -> Result<i32, uptown_funk::Trap> {
        Ok(number.value)
    }
}

#[test]
fn custom_type_add_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/custom_types_ref.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = Module::new(store.engine(), wasm).unwrap();
    let mut linker = Linker::new(&store);

    let empty = NumberState {
        saved_number: MyNumber { value: 1337 },
    };
    let instance_state = InstanceState {};
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();

    assert_eq!(test().is_ok(), true);
}
