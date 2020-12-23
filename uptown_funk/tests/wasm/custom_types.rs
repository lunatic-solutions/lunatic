#[link(wasm_import_module = "env")]
extern "C" {
    fn add(a: i32, b: i32) -> i32;
}

#[export_name = "test"]
pub extern "C" fn test() {
    let result = unsafe { add(2, 3) };
    assert_eq!(result, 5);
}
