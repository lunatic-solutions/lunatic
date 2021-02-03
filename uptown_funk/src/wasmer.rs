//! Wasmer specific definitions

use std::{collections::HashMap};

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
