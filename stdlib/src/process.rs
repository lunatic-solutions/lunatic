use std::mem::{forget, size_of, zeroed};
use std::ptr;

use crate::stdlib::drop;
use crate::{Channel, ProcessClosureSend};

mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn spawn(function_ptr: unsafe extern "C" fn(i64), argument: i64) -> u32;
        pub fn join(pid: u32);
    }
}

#[derive(Debug)]
pub struct SpawnError {}

/// A process consists of its own stack and heap. It can only share data with other processes by
/// sending it to them.
pub struct Process {
    id: u32,
}

impl Drop for Process {
    fn drop(&mut self) {
        // Decrement reference count of resource in the VM
        unsafe {
            drop(self.id);
        }
    }
}

impl Process {
    /// Spawn a new process. The passed closure can only capture copy types.
    pub fn spawn<F>(closure: F) -> Result<Process, SpawnError>
    where
        F: FnOnce() + ProcessClosureSend,
    {
        unsafe extern "C" fn spawn_small_capture<F>(closure_in_i64: i64)
        where
            F: FnOnce() + ProcessClosureSend,
        {
            let source = &closure_in_i64 as *const i64 as *const F;
            let mut f: F = zeroed();
            ptr::copy_nonoverlapping(source, &mut f, size_of::<F>());
            f();
        }

        unsafe extern "C" fn spawn_large_capture<F>(channel: i64)
        where
            F: FnOnce() + ProcessClosureSend,
        {
            let channel: Channel<F> = Channel::from_id(channel as u32);
            let f = channel.receive();
            f();
        }

        let id = if size_of::<F>() <= size_of::<i64>() {
            // If we can fit the environment directly in a 64 bit pointer do so.
            let source = &closure as *const F as *const i64;
            let mut closure_in_i64: i64 = 0;
            unsafe { ptr::copy_nonoverlapping(source, &mut closure_in_i64, size_of::<F>()) };
            forget(closure);
            unsafe { stdlib::spawn(spawn_small_capture::<F>, closure_in_i64) }
        } else {
            // We need to first send the environment between the processes before we can use the closure.
            let channel: Channel<F> = Channel::new(1);
            channel.send(closure);
            let id = unsafe { stdlib::spawn(spawn_large_capture::<F>, channel.id() as i64) };
            forget(channel);
            id
        };

        Ok(Self { id })
    }

    /// Wait on a process to finish.
    pub fn join(self) {
        unsafe {
            stdlib::join(self.id);
        };
        forget(self);
    }
}
