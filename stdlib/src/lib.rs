pub mod channel;
pub mod process;

pub use channel::Channel;
pub use process::Process;

mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn r#yield();
    }
}

pub fn yield_() {
    unsafe { stdlib::r#yield(); }
}