#[link(wasm_import_module = "env")]
extern "C" {
    fn create(n: i32) -> i32;
    fn value(index: i32) -> i32;
    fn add(a_index: i32, b_index: i32) -> i32;
    fn sum() -> i32;

}

#[export_name = "test"]
pub extern "C" fn test() {
    let a = unsafe { create(7) };
    let b = unsafe { create(13) };

    let get_a = unsafe { value(a) };
    assert_eq!(get_a, 7);

    let c = unsafe { add(a, b) };
    let get_c = unsafe { value(c) };
    assert_eq!(get_c, 20);

    unsafe { create(7) };

    let sum = unsafe { sum() };
    assert_eq!(sum, 47);
}
