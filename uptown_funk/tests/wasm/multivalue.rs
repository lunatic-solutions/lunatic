#[link(wasm_import_module = "env")]
extern "C" {
    fn leet(a: *mut f32, b: *mut i64) -> i32;
}

#[export_name = "test"]
pub extern "C" fn test() {
    let mut a: f32 = 0.0;
    let mut b: i64 = 0;
    let result = unsafe { leet(&mut a as *mut f32, &mut b as *mut i64) };
    assert_eq!(a, 1337.1337);
    assert_eq!(b, 1337);
    assert_eq!(result, 1337);
}
