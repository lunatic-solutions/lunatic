pub mod api;
pub mod state;
pub mod types;

#[cfg(any(
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android",
    target_os = "macos"
))]
mod unix;

#[cfg(any(target_os = "windows"))]
mod windows;
