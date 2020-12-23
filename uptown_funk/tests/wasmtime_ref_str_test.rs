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
    fn count_a(&self, words: &str) -> i32 {
        words.matches("a").count() as i32
    }

    fn add(&self, a: &str, b: &str, c: &mut [u8]) {
        c[..a.len()].copy_from_slice(a.as_bytes());
        c[a.len()..].copy_from_slice(b.as_bytes());
    }
}

#[test]
fn ref_str_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/ref_str.wasm")
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

    let test_count = instance
        .get_func("test_count")
        .unwrap()
        .get0::<()>()
        .unwrap();
    assert_eq!(test_count().is_ok(), true);

    let test_add = instance.get_func("test_add").unwrap().get0::<()>().unwrap();
    assert_eq!(test_add().is_ok(), true);
}
