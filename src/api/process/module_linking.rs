use crate::{api::default::DefaultApi, module::LunaticModule};
use uptown_funk::{Executor, FromWasm, HostFunctions, ToWasm};
use wasmtime::{Limits, Memory, MemoryType};

use super::api::ProcessState;

impl FromWasm<&mut ProcessState> for LunaticModule {
    type From = u32;

    fn from(
        state: &mut ProcessState,
        _: &impl Executor,
        module_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.modules.get(module_id) {
            Some(module) => Ok(module.clone()),
            None => Err(uptown_funk::Trap::new("LunaticModule not found")),
        }
    }
}

pub enum ModuleResult {
    Ok(LunaticModule),
    Err,
}

impl ToWasm<&mut ProcessState> for ModuleResult {
    type To = u32;

    fn to(
        state: &mut ProcessState,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            ModuleResult::Ok(listener) => Ok(state.modules.add(listener)),
            ModuleResult::Err => Ok(0),
        }
    }
}

#[derive(Clone)]
pub struct Import(pub String, pub LunaticModule);

impl ToWasm<&mut ProcessState> for Import {
    type To = u32;

    fn to(
        state: &mut ProcessState,
        _: &impl Executor,
        import: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.imports.add(import))
    }
}

pub struct Imports {
    module: LunaticModule,
    imports: Vec<Option<Import>>,
}

impl Imports {
    pub fn new(module: LunaticModule, imports: Vec<Option<Import>>) -> Self {
        Self { module, imports }
    }
}

impl HostFunctions for Imports {
    type Return = ();
    type Wrap = Self;

    fn split(self) -> (Self::Return, Self::Wrap) {
        ((), self)
    }

    fn add_to_linker<E>(imports: Self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        // Include default API
        let default_api = DefaultApi::new(None, imports.module.clone());
        DefaultApi::add_to_linker(default_api, executor.clone(), linker);

        // Allow overriding default imports
        linker.allow_shadowing(true);

        // For each import create a separate instance that will be used as an import namespace.
        for import in imports.imports {
            match import {
                Some(import) => {
                    let store = linker.store();
                    let mut parent_linker = wasmtime::Linker::new(&store);

                    // Create memory for parent
                    let memory_ty =
                        MemoryType::new(Limits::new(import.1.min_memory(), import.1.max_memory()));
                    let memory = Memory::new(&store, memory_ty);
                    parent_linker.define("lunatic", "memory", memory).unwrap();

                    let default_api = DefaultApi::new(None, import.1.clone());
                    DefaultApi::add_to_linker(default_api, executor.clone(), &mut parent_linker);
                    let instance = parent_linker
                        .instantiate(import.1.module().wasmtime().unwrap())
                        .unwrap();
                    linker.instance(&import.0, &instance).unwrap();
                }
                None => (),
            }
        }
    }
}
