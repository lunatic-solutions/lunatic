use wasmtime::Linker;

pub trait LunaticLib {
    fn functions(&self) -> Vec<&'static str>;
    fn add_to_linker(&self, yielder_ptr: usize, linker: &mut Linker);
}