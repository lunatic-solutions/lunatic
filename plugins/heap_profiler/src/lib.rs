mod api;
mod patch;

use walrus::Module;

#[link(wasm_import_module = "lunatic::plugin")]
extern "C" {
    fn take_module(offset: *mut u8);
    fn set_module(offset: *mut u8, offset_len: u32);
}

#[export_name = "lunatic_create_module_hook"]
pub extern "C" fn lunatic_create_module_hook(module_size: u64) {
    let mut module_buffer = vec![0; module_size as usize];
    // Get the module that is being compiled from host
    unsafe { take_module(module_buffer.as_mut_ptr()) }
    // Patch the module to report heap profiling information
    let mut module = Module::from_buffer(&module_buffer).unwrap();
    patch::patch(&mut module);
    // Put it back
    unsafe { set_module(module_buffer.as_mut_ptr(), module_buffer.len() as u32) }
}
