#[link(wasm_import_module = "env")]
extern "C" {
    fn return_7() -> i32;
    fn return_1_2_3(a: *mut i32, b: *mut i32) -> i32;
}

#[export_name = "test"]
pub extern "C" fn test() {
    let result = unsafe { return_7() };
    assert_eq!(result, 7);
}

#[export_name = "test_multivalue"]
pub extern "C" fn test_multivalue() {
    let mut a = 0;
    let mut b = 0;
    let result = unsafe { return_1_2_3(&mut a as *mut i32, &mut b as *mut i32) };
    assert_eq!(result, 1);
    assert_eq!(a, 2);
    assert_eq!(b, 3);
}
