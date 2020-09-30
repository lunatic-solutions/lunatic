mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    #[allow(improper_ctypes)]
    extern "C" {
        pub fn spawn(function_ptr: usize, argument: usize);
    }
}

pub fn spawn<F>(f: F)
where
    F: FnOnce() + Copy
{
    unsafe extern "C" fn spawn_wrapper<F>(function_ptr: usize)
    where
        F: FnOnce() + Copy
    {
        let f = std::ptr::read(function_ptr as *const F);
        f();
    }

    unsafe { stdlib::spawn(spawn_wrapper::<F> as usize, &f as *const F as usize) };
}