use std::mem::size_of;

use crate::Channel;

mod stdlib {
    #[link(wasm_import_module = "lunatic")]
    extern "C" {
        pub fn spawn(function_ptr: unsafe extern "C" fn(usize), argument: usize) -> i32;
        pub fn join(pid: i32);
    }
}

#[derive(Debug)]
pub struct SpawnError {}

pub struct Process {
    id: i32
}

impl Process {
    pub fn spawn<F>(closure: F) -> Result<Process, SpawnError>
    where
        F: FnOnce() + Copy + 'static
    {
        unsafe extern "C" fn spawn_without_capture<F>(function_ptr: usize)
        where
            F: FnOnce() + Copy + 'static
        {
            let f = std::ptr::read(function_ptr as *const F);
            f();
        }

        unsafe extern "C" fn spawn_with_capture<F>(channel: usize)
        where
            F: FnOnce() + Copy + 'static
        {
            let channel: Channel<F> = Channel::from_id(channel as i32);
            let f = channel.receive();
            f();
        }

        let id = if size_of::<F>() == 0 {
            // If no environment is captured pass the closure pointer directly.
            unsafe { stdlib::spawn(spawn_without_capture::<F>, &closure as *const F as usize) }
        } else {
            // We need to first send the environment between the processes before we can use the closure.
            let channel: Channel<F> = Channel::new(1);
            channel.send(closure);
            unsafe { stdlib::spawn(spawn_with_capture::<F>, channel.id() as usize) }
        };

        if id > -1 {
            Ok(Self { id })
        } else {
            Err(SpawnError {})
        }
    }

    pub fn join(&self) {
        unsafe { stdlib::join(self.id); }
    }
}
