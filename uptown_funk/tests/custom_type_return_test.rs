use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

use std::cell::RefCell;
use std::fs::read;
use std::rc::Rc;

struct InstanceState {
    memory: Rc<RefCell<Option<Memory>>>,
}

impl InstanceEnvironment for InstanceState {
    fn wasm_memory(&self) -> &mut [u8] {
        let memory_ref = self.memory.borrow();
        // Transmute to outlive RefCell borrow
        unsafe { std::mem::transmute(memory_ref.as_ref().unwrap().data_unchecked_mut()) }
    }
}

struct Empty {}

#[host_functions(namespace = "env")]
impl Empty {
    fn return_7(&self) -> MyNumber {
        MyNumber { value: 7 }
    }

    fn return_1_2_3(&self) -> (MyNumber, MyNumber, MyNumber) {
        (
            MyNumber { value: 1 },
            MyNumber { value: 2 },
            MyNumber { value: 3 },
        )
    }
}

struct MyNumber {
    value: i32,
}

impl uptown_funk::ToWasmU32 for MyNumber {
    type State = Empty;

    fn to_u32<InstanceState>(
        _: &mut Self::State,
        _: &InstanceState,
        number: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(number.value as u32)
    }
}

#[test]
fn custom_type_return_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/custom_types_return.wasm")
        .expect("Wasm file not found. Did you run ./build.sh inside the tests/wasm/ folder?");
    let module = Module::new(store.engine(), wasm).unwrap();
    let mut linker = Linker::new(&store);

    let empty = Empty {};
    let memory = Rc::new(RefCell::new(None));
    let instance_state = InstanceState {
        memory: memory.clone(),
    };
    empty.add_to_linker(instance_state, &mut linker);

    let instance = linker.instantiate(&module).unwrap();
    {
        // Capture instance memory.
        let instance_memory = instance.get_memory("memory").unwrap();
        *memory.borrow_mut() = Some(instance_memory);
    }

    let test = instance.get_func("test").unwrap().get0::<()>().unwrap();
    assert_eq!(test().is_ok(), true);

    let test_mutlivalue = instance
        .get_func("test_multivalue")
        .unwrap()
        .get0::<()>()
        .unwrap();
    assert_eq!(test_mutlivalue().is_ok(), true);
}
