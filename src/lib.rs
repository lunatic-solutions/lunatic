#![feature(unsafe_cell_get_mut)]

pub mod channel;
pub mod normalisation;
pub mod process;
pub mod wasi;

use crossbeam::queue::SegQueue;
use lazy_static::lazy_static;
use std::sync::{Mutex, RwLock};

lazy_static! {
    /// All resources (open files, sockets, processes, ...) held by the runtime.
    pub static ref RESOURCES: GlobalResources = GlobalResources::new();
}

/// This structure is used to keep track of all resources allocated by the Lunatic runtime.
/// Resources are kept in a vector, are opaque to the WebAssembly guest code and are passed between host
/// and guest only as indices in the vector. New resources can be created from any process (thread) running,
/// this requires a RwLock around the vector. Because of internal implementation details each resource
/// currently also requires a Mutex wrapper, but eventually will be replaced by atomic reference counts in
/// only cloneable resources.
///
/// A set of free slots is also maintained to allow best utilisation of the vector.
///
/// In an ideal scenario, all resources created by Lunatic would be passed as reference types to guest
/// code and the guest would be in charge of managing them and not Lunatic's runtime. Sadly, most of the
/// programming languages (Rust, C/C++, ...) used to create WebAssembly modules don't have a way of dealing
/// with [externrefs][1]. To make it easier from those languages to hold resources, all of them are
/// presented as integer indices to guest code.
///
/// [1]: https://github.com/WebAssembly/reference-types/blob/master/proposals/reference-types/Overview.md
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

    /// Add resource to the vector of all resources.
    pub fn add(&self, new_resource: Resource) -> usize {
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

    /// If it's a cloneable resource clone it.
    pub fn clone(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        match resource.as_mut() {
            Some(Resource::Cloneable(resrouce)) => {
                assert!(resrouce.count > 0);
                resrouce.count += 1;
            }
            _ => panic!("Can't clone owned resources"),
        }
    }

    /// Drop resource.
    /// If it's cloneable decrement the reference count and after reaching 0 free the whole process.
    pub fn drop(&self, index: usize) {
        let resources = self.resources.read().unwrap();
        let resource_mutex = resources.get(index).unwrap();
        let mut resource = resource_mutex.lock().unwrap();

        assert!(resource.is_some());

        let count = match resource.as_mut() {
            Some(Resource::Cloneable(resrouce)) => {
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
                Resource::Owned(ResourceTypeOwned::Process(process)) => {
                    process.take_task().detach()
                }
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

/// There are two main kinds of resources:
/// * Cloneable - Multiple references can exist at the same time to this resource.
/// * Owned - Only one reference can exist to this type of resource.
pub enum Resource {
    Cloneable(ResourceRc),
    Owned(ResourceTypeOwned),
}

#[derive(Debug)]
pub struct ResourceRc {
    resource: ResourceTypeCloneable,
    count: usize,
}

pub enum ResourceTypeOwned {
    Process(process::Process),
    File(smol::fs::File),
}

#[derive(Debug, Clone)]
pub enum ResourceTypeCloneable {
    Channel(channel::Channel),
}
