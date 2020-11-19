pub mod api;
mod permissions;

use anyhow::Result;
use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;
use lazy_static::lazy_static;
use smol::{Executor, Task};
use wasmtime::{Engine, Memory, Module, Val};

use crate::linker::LunaticLinker;
use permissions::ProcessPermissions;

use std::cell::RefCell;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::rc::Rc;

lazy_static! {
    static ref WORMHOLE_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
}

pub type AsyncYielderCast<'a> = AsyncYielder<'a, Result<()>>;

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    /// (table index, argument1, argument2)
    TableIndex((i32, i32, i64)),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances (once the WASM multi-threading proposal is supported in Wasmtime).
#[derive(Clone)]
pub enum MemoryChoice {
    Existing(Memory),
    New(u32),
}

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a raw pointer to Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe to have a
/// raw pointer to its memory content and only use it inside of host functions.
#[derive(Clone)]
pub struct ProcessEnvironment {
    engine: Engine,
    module: Module,
    permissions: ProcessPermissions,
    memory: *mut u8,
    yielder: usize,
    /// If the  language running on top of Lunatic doesn't support externrefs natively, during the
    /// normalisation phase an externref table is added to store the externrefs. This externrefs are
    /// accessed by the guest language through indices. To keep track of empty slots in this externref
    /// table a tuple (capacity, vector) is used.
    /// The capacity represents the capacity of the externref table & the vector holds all currently
    /// free slots in the table
    externref_free_slots: Rc<RefCell<(i32, Vec<i32>)>>,
}

impl ProcessEnvironment {
    pub fn new(engine: Engine, module: Module, memory: *mut u8, yielder: usize) -> Self {
        // Initialise externref table with 4 free slots.
        let externref_free_slots = Rc::new(RefCell::new((4, Vec::from([3, 2, 1, 0]))));
        Self {
            engine,
            module,
            permissions: ProcessPermissions::current_dir(),
            memory,
            yielder,
            externref_free_slots,
        }
    }

    /// Get the next free slot in the externrefs table. If no slots are available double the size of the
    /// table and try again.
    pub fn get_externref_free_slot(&self) -> i32 {
        let mut externref_free_slots = self.externref_free_slots.borrow_mut();
        match externref_free_slots.1.pop() {
            Some(free_slot) => free_slot,
            None => {
                // If capacity of externrefs reached, double it and pop a new element.
                let capacity = externref_free_slots.0;
                let new_slots = ((capacity + 1)..(capacity * 2)).rev();
                externref_free_slots.0 *= 2;
                externref_free_slots.1.extend(new_slots);
                capacity
            }
        }
    }

    /// Mark a slot as free in the externrefs table.
    pub fn set_externref_free_slot(&self, free_slot: i32) {
        self.externref_free_slots.borrow_mut().1.push(free_slot);
    }

    /// Run an async future and return the output when done.
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder =
            unsafe { std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielderCast>) };
        yielder.async_suspend(future)
    }

    pub fn memory(&self) -> *mut u8 {
        self.memory
    }

    pub fn engine(&self) -> Engine {
        self.engine.clone()
    }

    pub fn module(&self) -> Module {
        self.module.clone()
    }
}

/// A lunatic process represents an actor.
pub struct Process {
    task: RefCell<Option<Task<Result<()>>>>,
}

impl Process {
    pub fn take_task(&self) -> Option<Task<Result<()>>> {
        self.task.borrow_mut().take()
    }

    /// Spawn a new process.
    pub fn spawn(
        engine: Engine,
        module: Module,
        function: FunctionLookup,
        memory: MemoryChoice,
    ) -> Self {
        let process = WORMHOLE_POOL.with_tls(
            [&wasmtime_runtime::traphandlers::tls::PTR],
            move |yielder| {
                let yielder_ptr = &yielder as *const AsyncYielderCast as usize;

                let linker = LunaticLinker::new(engine, module, yielder_ptr, memory)?;
                let instance = linker.instance()?;

                match function {
                    FunctionLookup::Name(name) => {
                        let func = instance.get_func(name).unwrap();
                        let now = std::time::Instant::now();
                        func.call(&[])?;
                        println!("Elapsed time: {} ms", now.elapsed().as_millis());
                    }
                    FunctionLookup::TableIndex((index, argument1, argument2)) => {
                        let func = instance.get_func("lunatic_spawn_by_index").unwrap();
                        func.call(&[Val::from(index), Val::from(argument1), Val::from(argument2)])?;
                    }
                }

                Ok(())
            },
        );

        let task = EXECUTOR.spawn(async {
            let mut process = process?;
            let result = (&mut process).await.unwrap();
            WORMHOLE_POOL.recycle(process);
            result
        });

        Self {
            task: RefCell::new(Some(task)),
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Don't cancel process on drop if still running
        match self.take_task() {
            Some(task) => task.detach(),
            None => (),
        };
    }
}
