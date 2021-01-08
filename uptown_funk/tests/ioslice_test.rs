use uptown_funk::{host_functions, HostFunctions, InstanceEnvironment};
use wasmtime::*;

use std::cell::RefCell;
use std::fs::read;
use std::io::IoSliceMut;
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
    fn vectored_read(&self, bufs: &mut [IoSliceMut<'_>]) {
        bufs.iter_mut().enumerate().for_each(|(i, buf)| {
            buf.copy_from_slice(&[i as u8; 8][..]);
        });
    }

    fn vectored_write(&self, _bufs: &[std::io::IoSlice<'_>]) {}
}

#[test]
fn ioslice_test() {
    let store = Store::default();
    let wasm = read("tests/wasm/ioslices.wasm")
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

    let test_mut_ioslice = instance
        .get_func("test_mut_ioslice")
        .unwrap()
        .get0::<()>()
        .unwrap();

    assert_eq!(test_mut_ioslice().is_ok(), true);
}
