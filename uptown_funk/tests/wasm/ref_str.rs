#[link(wasm_import_module = "env")]
extern "C" {
    fn count_a(str_ptr: *const u8, str_len: usize) -> i32;
    fn add(
        a_ptr: *const u8,
        a_len: usize,
        b_ptr: *const u8,
        b_len: usize,
        r_ptr: *mut u8,
        r_len: usize,
    );
}

#[export_name = "test_count"]
pub extern "C" fn test_count() {
    let input = "Hallo warld; aaaa";
    let result = unsafe { count_a(input.as_ptr(), input.len()) };
    assert_eq!(result, 6);
}

#[export_name = "test_add"]
pub extern "C" fn test_add() {
    let a = "Hello ";
    let b = "world";
    let mut result: [u8; 11] = [0; 11];
    unsafe {
        add(
            a.as_ptr(),
            a.len(),
            b.as_ptr(),
            b.len(),
            result.as_mut_ptr(),
            result.len(),
        )
    };
    assert_eq!(result, "Hello world".as_bytes());
}
