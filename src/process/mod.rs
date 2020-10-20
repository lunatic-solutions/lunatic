pub mod channel;
pub mod creator;
pub mod imports;

use async_wormhole::pool::OneMbAsyncPool;
use async_wormhole::AsyncYielder;

use anyhow::Result;
use crossbeam::queue::SegQueue;
use lazy_static::lazy_static;
use smol::{fs, Executor, Task};
use wasmtime::{Engine, Module};

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
    task: Task<Result<()>>,
}

impl Process {
    pub fn from(task: Task<Result<()>>) -> Self {
        Self { task }
    }

    pub fn take_task(self) -> impl Future {
        self.task
    }
}

/// Holds all resources that are sendable between processes (file descriptors, channels, other processes, ...).
/// Each process has his own heap space and it's not possible to keep track of allocated processes (especially
/// clonable) in the guest space.
pub struct GlobalResources {
    resources: RwLock<Vec<Mutex<Option<Resource>>>>,
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
                let resource_mutex = resources.get(free_index).unwrap();
                let mut resource = resource_mutex.lock().unwrap();

                assert!(resource.is_none());

                *resource = Some(new_resource);
                free_index
            }
            None => {
                let mut resources = self.resources.write().unwrap();
                resources.push(Mutex::new(Some(new_resource)));
                resources.len() - 1
            }
        }
    }

    /// If it's a clonable resource clone it.
    pub fn clone(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        match resource.as_mut() {
            Some(Resource::Clonable(resrouce)) => {
                assert!(resrouce.count > 0);
                resrouce.count += 1;
            }
            _ => panic!("Can't clone owned resources"),
        }
    }

    /// Drop resource.
    /// If it's clonable decrement the reference count and after reaching 0 free the whole process.
    pub fn drop(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        let count = match resource.as_mut() {
            Some(Resource::Clonable(resrouce)) => {
                assert!(resrouce.count > 0);
                resrouce.count -= 1;
                resrouce.count
            }
            Some(Resource::Owned(_resrouce)) => 0,
            None => unreachable!("Assert stops us from getting here"),
        };

        if count == 0 {
            let drop = resource.take();
            match drop.unwrap() {
                // If we are dropping a process we need to detach it first or it will be canceled.
                Resource::Owned(ResourceTypeOwned::Process(process)) => process.task.detach(),
                _ => (),
            };
            self.free.push(index);
        }
    }

    /// Runs function `f` with access to the resource.
    pub fn with_resource<F: FnOnce(&mut Resource)>(&self, index: usize, f: F) {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        f(resource.as_mut().unwrap());
    }

    /// Take resource
    pub fn take(&self, index: usize) -> Resource {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        resource.take().unwrap()
    }
}

pub enum Resource {
    Clonable(ResourceRc),
    Owned(ResourceTypeOwned),
}

#[derive(Debug)]
pub struct ResourceRc {
    resource: ResourceTypeClonable,
    count: usize,
}

pub enum ResourceTypeOwned {
    Process(Process),
    File(fs::File),
}

#[derive(Debug, Clone)]
pub enum ResourceTypeClonable {
    Channel(channel::Channel),
}
