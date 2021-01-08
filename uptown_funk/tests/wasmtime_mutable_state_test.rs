use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

use std::fs::read;

struct InstanceState {}

impl InstanceEnvironment for InstanceState {
    fn wasm_memory(&self) -> &mut [u8] {
        &mut []
    }
}

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

impl uptown_funk::FromWasmU32<'_> for MyNumber {
    type State = ArrayState;

    fn from_u32<InstanceState>(
        state: &mut Self::State,
        _: &InstanceState,
        index: u32,
    ) -> Result<Self, uptown_funk::Trap> {
        match state.vec.get(index as usize) {
            Some(number) => Ok(number.clone()),
            None => Err(uptown_funk::Trap::new("Number not found")),
        }
    }
}

impl uptown_funk::ToWasmU32 for MyNumber {
    type State = ArrayState;

    fn to_u32<InstanceState>(
        state: &mut Self::State,
        _: &InstanceState,
        number: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        let index = state.vec.len();
        state.vec.push(number);
        Ok(index as u32)
    }
}

#[test]
fn mutable_state_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/mutable_state.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = Module::new(store.engine(), wasm).unwrap();
    let mut linker = Linker::new(&store);

    let empty = ArrayState { vec: Vec::new() };
    let instance_state = InstanceState {};
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();

    assert_eq!(test().is_ok(), true);
}
