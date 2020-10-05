#![feature(optin_builtin_traits, negative_impls)]

mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    #[allow(improper_ctypes)]
    extern "C" {
        pub fn spawn(function_ptr: usize, argument: usize) -> usize;
    }
}

pub fn spawn<F>(f: F) -> usize
where
    F: FnOnce() + Copy + 'static
{
    unsafe extern "C" fn spawn_wrapper<F>(function_ptr: usize)
    where
        F: FnOnce()
    {
        let f = std::ptr::read(function_ptr as *const F);
        f();
    }

    unsafe { stdlib::spawn(spawn_wrapper::<F> as usize, &f as *const F as usize) }
}


pub unsafe auto trait ProcessSend {}

impl<F> !ProcessSend for &F where F: FnOnce() {}

impl !ProcessSend for &i32 {}
impl !ProcessSend for &mut i32 {}
impl !ProcessSend for *const i32 {}
impl !ProcessSend for *mut i32 {}
