use lunatic::codemod::reduction_counting::insert_reduction_counting;
use wat::parse_str;
use wasmparser::validate;

#[test]
pub fn insert_reduction_counter_in_function_test() {
    let wat = r#"
        (module
            (func (export "hello") (result i32)
                i32.const 45
            )
        )
    "#;
    let wasm = parse_str(wat).unwrap();
    let wasm = insert_reduction_counting(&wasm);
    let result = validate(&wasm);
    println!("{:?}", &result);
    assert!(result.is_ok());
}