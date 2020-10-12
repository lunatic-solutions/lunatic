pub mod channel;
pub mod creator;
pub mod imports;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use wasmtime::{Engine, Module};
use smol::{Task, Executor};
use anyhow::Result;
use lazy_static::lazy_static;

use std::future::Future;
use std::sync::RwLock;
use std::rc::Rc;
use std::cell::RefCell;
use std::mem::ManuallyDrop;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
}

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a raw pointer to Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe to have a
/// raw pointer to its memory content and only use it inside of host functinos.
#[derive(Clone)]
pub struct ProcessEnvironment {
    engine: Engine,
    module: Module,
    memory: *mut u8,
    yielder: usize,
    processes: Rc<RefCell<State<Process>>>
}

impl ProcessEnvironment {
    pub fn new(engine: Engine, module: Module, memory: *mut u8, yielder: usize) -> Self {
        let processes = Rc::new(RefCell::new(State::new(20_000)));
        Self { engine, module, memory, yielder, processes }
    }

    /// Run an async future and return the output when done.
    pub fn async_<Fut, R>(&self, future: Fut) -> R
    where
        Fut: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder =
            unsafe { std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<R>>) };
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

pub struct Process {
    task: Task<Result<()>>
}

impl Process {
    pub fn from(task: Task<Result<()>>) -> Self {
        Self { task }
    }

    pub fn mut_task(&mut self) -> &mut Task<Result<()>> {
        &mut self.task
    }
}


struct State<T> {
    occupied: Vec<Option<T>>,
    free: Vec<usize>
}

impl<T> State<T> {
    fn new(capacity: usize) -> Self {
        Self {
            occupied: Vec::with_capacity(capacity),
            free: (0..capacity).rev().collect()
        }
    }

    fn insert(&mut self, value: T) -> Option<usize> {
        let free_slot = self.free.pop()?;
        if free_slot == self.occupied.len() {
            self.occupied.push(Some(value));
        } else {
            self.occupied[free_slot] = Some(value);
        }
        Some(free_slot)
    }

    fn delete(&mut self, slot: usize) {
        let _drop = self.occupied[slot].take();
        self.free.push(slot);
    }

    fn get_mut(&mut self, slot: usize) -> &mut Option<T> {
        &mut self.occupied[slot]
    }

    fn get(&self, slot: usize) -> &Option<T> {
        &self.occupied[slot]
    }
}

struct GlobalState<T> {
    state: RwLock<State<T>>
}