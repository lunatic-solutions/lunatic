use wasmer::Exports;

pub trait LunaticLib {
    fn functions(&self) -> Vec<&'static str>;
    fn add_to_imports(&self, yielder_ptr: usize, linker: &mut Exports);
}