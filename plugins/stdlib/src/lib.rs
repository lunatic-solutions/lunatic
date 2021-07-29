/*!
This plugin will add a `lunatic_call_indirect(table_index: u32)` function to the module.

`lunatic_call_indirect` allows you to call a "private" function by table index. It is used to spawn
new processes just by function table index, making it convenient in some guest languages to spawn a
new process just by function "pointer".
*/

#[link(wasm_import_module = "lunatic::plugin")]
extern "C" {
    fn add_function(
        type_index: u32,
        func_local_ptr: *const u8,
        func_local_len: u32,
        func_body_ptr: *const u8,
        func_body_len: u32,
        id_ptr: *mut u64,
    ) -> u32;

    fn add_function_type(
        param_types_ptr: *const u8,
        param_types_len: u32,
        ret_types_ptr: *const u8,
        ret_types_len: u32,
        id_ptr: *mut u64,
    ) -> u32;

    fn add_function_export(name_str: *const u8, name_str_len: u32, function_id: u32);
}

#[export_name = "lunatic_create_module_hook"]
pub extern "C" fn lunatic_create_module_hook() {
    // Signature of "to be called" function in the table
    // () -> ()
    let mut ptr_type_index: u64 = 0;
    let result = unsafe {
        add_function_type(
            [].as_ptr(),
            0,
            [].as_ptr(),
            0,
            &mut ptr_type_index as *mut u64,
        )
    };
    assert_eq!(result, 0);

    // Signature of `lunatic_call_indirect`
    // (i32) -> ()
    let mut type_index: u64 = 0;
    let result = unsafe {
        add_function_type(
            [0x7F].as_ptr(), // [I32]
            1,
            [].as_ptr(),
            0,
            &mut type_index as *mut u64,
        )
    };
    assert_eq!(result, 0);

    // All integers in WebAssembly are LEB128 encoded
    let mut ptr_type_index_leb128 = [0; 4];
    let mut writable = &mut ptr_type_index_leb128[..];
    let size = leb128::write::unsigned(&mut writable, ptr_type_index).unwrap();

    // `lunatic_call_indirect` body:
    //   local.get 0
    //   call_indirect ptr_type_index, 0
    //   end
    let mut function_body = vec![0x20, 0x0, 0x11];
    function_body.extend(&ptr_type_index_leb128[..size]);
    function_body.extend([0, 0x0B]);

    let mut function_index: u64 = 0;
    let result = unsafe {
        add_function(
            type_index as u32,
            [].as_ptr(),
            0,
            function_body.as_ptr(),
            function_body.len() as u32,
            &mut function_index as *mut u64,
        )
    };
    assert_eq!(result, 0);

    // Export `lunatic_call_indirect`
    let name = "lunatic_call_indirect";
    unsafe { add_function_export(name.as_ptr(), name.len() as u32, function_index as u32) };
}
