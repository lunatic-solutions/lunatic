#![feature(optin_builtin_traits, negative_impls)]

//! This library contains higher level wrappers for low level Lunatic syscalls.
//!
//! Currently it requires nightly.
//!
//! ### Example
//!
//! Create 100k processes and calculate the power of numbers then send the results back to the original process.
//!
//! ```rust
//! use lunatic::{Channel, Process};
//!
//! fn main() {
//!     let channel = Channel::new(0);
//!
//!     for i in 0..100_000 {
//!         let x = channel.clone();
//!         Process::spawn(move || {
//!             x.send((i, power(i)));
//!         })
//!         .unwrap();
//!     }
//!
//!     for _ in 0..100_000 {
//!         let (i, power) = channel.receive();
//!         println!("Power of {} is {}", i, power);
//!     }
//! }
//!
//! fn power(a: i32) -> i32 {
//!     a * a
//! }
//!
//! ```
//!
//! Compile your app to a WebAssembly target:
//!
//! ```
//! cargo build --release --target=wasm32-wasi
//! ```
//!
//! and run it with
//!
//! ```
//! lunatic target/wasm32-wasi/release/<name>.wasm
//! ```

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

/// Sending data to another process requires copying it into an independent buffer.
/// It's only safe to do so with copy types without serialisation.
pub unsafe auto trait ProcessClosureSend {}

impl<T> !ProcessClosureSend for &T where T: ?Sized {}
impl<T> !ProcessClosureSend for &mut T where T: ?Sized {}
impl<T> !ProcessClosureSend for *const T where T: ?Sized {}
impl<T> !ProcessClosureSend for *mut T where T: ?Sized {}
