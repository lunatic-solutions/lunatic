#[link(wasm_import_module = "env")]
extern "C" {
    fn write(value: f64, destination: *mut f64);
}

#[export_name = "test"]
pub extern "C" fn test() {
    let mut destination: f64 = 0.0;
    unsafe { write(0.64, &mut destination as *mut f64) };
    assert_eq!(destination, 0.64);
}
