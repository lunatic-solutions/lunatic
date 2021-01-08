//! Wasmer specific definitions

use super::{InstanceEnvironment, StateWrapper};
use std::{collections::HashMap, rc::Rc};

pub struct WasmerStateWrapper<S, E: InstanceEnvironment> {
    state_wrapper: Rc<StateWrapper<S, E>>,
}

impl<S, E: InstanceEnvironment> WasmerStateWrapper<S, E> {
    pub fn new(state_wrapper: StateWrapper<S, E>) -> Self {
        Self {
            state_wrapper: Rc::new(state_wrapper),
        }
    }

    pub fn state_wrapper(&self) -> &StateWrapper<S, E> {
        &self.state_wrapper
    }
}

unsafe impl<S, E: InstanceEnvironment> Send for WasmerStateWrapper<S, E> {}
unsafe impl<S, E: InstanceEnvironment> Sync for WasmerStateWrapper<S, E> {}

impl<S, E: InstanceEnvironment> Clone for WasmerStateWrapper<S, E> {
    fn clone(&self) -> Self {
        WasmerStateWrapper {
            state_wrapper: self.state_wrapper.clone(),
        }
    }
}

impl<S, E: InstanceEnvironment> wasmer::WasmerEnv for WasmerStateWrapper<S, E> {
    fn init_with_instance(&mut self, _: &wasmer::Instance) -> Result<(), wasmer::HostEnvInitError> {
        Ok(())
    }
}

pub struct WasmerLinker {
    imports: HashMap<String, HashMap<String, wasmer::Export>>,
}

impl WasmerLinker {
    pub fn new() -> Self {
        Self {
            imports: HashMap::new(),
        }
    }

    pub fn add<S: Into<String>>(&mut self, module: S, name: S, export: wasmer::Export) {
        let name = name.into();
        self.imports
            .entry(module.into())
            .and_modify(|m| {
                m.insert(name.clone(), export.clone());
            })
            .or_insert_with(|| {
                let mut m = HashMap::new();
                m.insert(name, export);
                m
            });
    }
}

impl wasmer::Resolver for WasmerLinker {
    fn resolve(&self, _index: u32, module: &str, name: &str) -> Option<wasmer::Export> {
        match self.imports.get(module.into()) {
            Some(occupied) => occupied.get(name.into()).map(ToOwned::to_owned),
            None => None,
        }
    }
}
