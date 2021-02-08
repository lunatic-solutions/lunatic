mod env1 {
    #[link(wasm_import_module = "env1")]
    extern "C" {
        pub fn add(a: i32, b: i32) -> i32;
    }
}

mod env2 {
    #[link(wasm_import_module = "env2")]
    extern "C" {
        pub fn add(a: i32, b: i32) -> i32;
    }
}

mod env3 {
    #[link(wasm_import_module = "env3")]
    extern "C" {
        pub fn add(a: i32, b: i32) -> i32;
    }
}

#[export_name = "test"]
pub extern "C" fn test() {
    let result = unsafe { env1::add(2, 3) };
    assert_eq!(result, 5);
    let result = unsafe { env2::add(2, 3) };
    assert_eq!(result, 5);
    let result = unsafe { env3::add(2, 3) };
    assert_eq!(result, 5);
}
