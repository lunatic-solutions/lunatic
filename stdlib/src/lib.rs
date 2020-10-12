#![feature(optin_builtin_traits, negative_impls)]

mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn spawn(function_ptr: unsafe extern "C" fn(usize), argument: usize) -> i32;
        pub fn join(pid: usize);
    }
}

pub fn spawn<F>(f: F) -> Option<usize>
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

    let pid = unsafe { stdlib::spawn(spawn_wrapper::<F>, &f as *const F as usize) };
    if pid > -1 {
        Some(pid as usize)
    } else {
        None
    }
}

pub fn join(pid: usize) {
    unsafe { stdlib::join(pid); }
}


// pub unsafe auto trait ProcessSend {}

// impl<F> !ProcessSend for &F where F: FnOnce() {}

// impl !ProcessSend for &i32 {}
// impl !ProcessSend for &mut i32 {}
// impl !ProcessSend for *const i32 {}
// impl !ProcessSend for *mut i32 {}
