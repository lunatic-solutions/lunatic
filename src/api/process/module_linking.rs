use crate::{api::default::DefaultApi, linker::wasmtime_engine, module::LunaticModule};
use uptown_funk::{Executor, FromWasm, HostFunctions, ToWasm};

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

pub enum LunaticModuleResult {
    Ok(LunaticModule),
    Err(String),
}

impl ToWasm<&mut ProcessState> for LunaticModuleResult {
    type To = u32;

    fn to(
        state: &mut ProcessState,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            LunaticModuleResult::Ok(listener) => Ok(state.modules.add(listener)),
            LunaticModuleResult::Err(_err) => Ok(0),
        }
    }
}

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

pub struct Imports<'a>(pub Vec<Option<&'a Import>>);

impl<'a> HostFunctions for Imports<'a> {
    type Return = ();
    type Wrap = Self;

    fn split(self) -> (Self::Return, Self::Wrap) {
        ((), self)
    }

    fn add_to_linker<E>(imports: Self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        // Allow overriding default imports
        linker.allow_shadowing(true);

        // For each import create a separate instance that will be used as an import namespace.
        for import in imports.0 {
            match import {
                Some(import) => {
                    let engine = wasmtime_engine();
                    let store = wasmtime::Store::new(&engine);
                    let mut parent_linker = wasmtime::Linker::new(&store);
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
