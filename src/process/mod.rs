pub mod channel;
pub mod creator;
pub mod imports;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use anyhow::Result;
use crossbeam::queue::SegQueue;
use lazy_static::lazy_static;
use smol::{Executor, Task};
use wasmtime::{Engine, Module};

use std::cell::{RefCell, UnsafeCell};
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::sync::{Mutex, RwLock};

pub type AsyncYielderCast<'a> = AsyncYielder<'a, Result<()>>;

lazy_static! {
    static ref ASYNC_POOL: OneMbAsyncPool = OneMbAsyncPool::new(128);
    pub static ref EXECUTOR: Executor<'static> = Executor::new();
    pub static ref RESOURCES: GlobalResources = GlobalResources::new();
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
}

impl ProcessEnvironment {
    pub fn new(engine: Engine, module: Module, memory: *mut u8, yielder: usize) -> Self {
        Self {
            engine,
            module,
            memory,
            yielder,
        }
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

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Process").finish()
    }
}

// We cheat here, Task can't be cloned, but we just take it away from the parent.
// As we only expect join(process) to be called once, this should be fine.
impl Clone for Process {
    fn clone(&self) -> Self {
        Process {
            task: RefCell::new(self.task.borrow_mut().take()),
        }
    }
}

impl Process {
    pub fn from(task: Task<Result<()>>) -> Self {
        Self {
            task: RefCell::new(Some(task)),
        }
    }

    pub fn take(&mut self) -> Option<Task<Result<()>>> {
        self.task.get_mut().take()
    }
}

pub struct GlobalResources {
    resources: RwLock<Vec<ResourceRc>>,
    free: SegQueue<usize>,
}

unsafe impl Sync for GlobalResources {}

impl GlobalResources {
    pub fn new() -> Self {
        Self {
            resources: RwLock::new(Vec::new()),
            free: SegQueue::new(),
        }
    }

    pub fn create(&self, new_resource: Resource) -> usize {
        match self.free.pop() {
            Some(free_index) => {
                let resources = self.resources.read().unwrap();
                let resource_rc = resources.get(free_index).unwrap();
                let mut ref_count = resource_rc.count.lock().unwrap();

                assert!(resource_rc.get_mut().is_none());
                assert_eq!(*ref_count, 0);

                *resource_rc.get_mut() = Some(new_resource);
                *ref_count += 1;
                free_index
            }
            None => {
                let mut resources = self.resources.write().unwrap();
                let resource_rc = ResourceRc {
                    resource: UnsafeCell::new(Some(new_resource)),
                    count: Mutex::new(1),
                };
                resources.push(resource_rc);
                resources.len() - 1
            }
        }
    }

    pub fn clone(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_rc = resources.get(index).unwrap();
        let mut ref_count = resource_rc.count.lock().unwrap();

        assert!(resource_rc.get_mut().is_some());
        assert!(*ref_count > 0);

        *ref_count += 1;
    }

    pub fn drop(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_rc = resources.get(index).unwrap();
        let mut ref_count = resource_rc.count.lock().unwrap();

        assert!(resource_rc.get_mut().is_some());
        assert!(*ref_count > 0);

        *ref_count -= 1;

        if *ref_count == 0 {
            let drop = resource_rc.get_mut().take();
            match drop.unwrap() {
                // If we are dropping a process we need to detach it first or it will be canceled.
                Resource::Process(mut process) => {
                    // If we joined the process is gone already.
                    match process.take() {
                        Some(task) => task.detach(),
                        None => (), // Task already consumed
                    }
                }
                _ => (),
            };
            self.free.push(index);
        }
    }

    pub fn get(&self, index: usize) -> Resource {
        let resources = self.resources.read().unwrap();
        let resource_rc = resources.get(index).unwrap();

        assert!(resource_rc.get_mut().is_some());

        let resrouce = resource_rc.get_mut().clone().unwrap();
        resrouce
    }
}

#[derive(Debug)]
pub struct ResourceRc {
    resource: UnsafeCell<Option<Resource>>,
    count: Mutex<usize>,
}

impl ResourceRc {
    pub fn get_mut(&self) -> &mut Option<Resource> {
        unsafe { &mut *self.resource.get() }
    }
}

#[derive(Debug, Clone)]
pub enum Resource {
    Process(Process),
    Channel(channel::Channel),
}
