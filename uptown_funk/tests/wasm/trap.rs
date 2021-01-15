#[link(wasm_import_module = "env")]
extern "C" {
    fn trap() -> i32;
}

#[export_name = "test"]
pub extern "C" fn test() {
    unsafe { trap() };
}
