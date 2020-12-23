#[link(wasm_import_module = "env")]
extern "C" {
    fn return_1337(a: i32) -> i32;
}

#[export_name = "test"]
pub extern "C" fn test() {
    let result = unsafe { return_1337(1) };
    assert_eq!(result, 1337);
}
