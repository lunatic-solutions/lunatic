#![feature(optin_builtin_traits, negative_impls)]

pub mod channel;
pub mod process;

pub use channel::Channel;
pub use process::Process;

pub mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn clone(channel: u32);
        pub fn drop(channel: u32);
        pub fn r#yield();
    }
}

pub fn yield_() {
    unsafe {
        stdlib::r#yield();
    }
}

/// Sending data to another process requires copying it into an independent buffer adn then reading
///
pub unsafe auto trait ProcessClosureSend {}

impl<T> !ProcessClosureSend for &T where T: ?Sized {}
impl<T> !ProcessClosureSend for &mut T where T: ?Sized {}
impl<T> !ProcessClosureSend for *const T where T: ?Sized {}
impl<T> !ProcessClosureSend for *mut T where T: ?Sized {}
