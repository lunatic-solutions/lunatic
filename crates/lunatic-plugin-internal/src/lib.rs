use std::{any::Any, ffi::OsStr, sync::Arc};

use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use wasmtime::Linker;

type PluginIDFn = unsafe extern "C" fn() -> &'static str;
type InitFn = unsafe extern "C" fn() -> Box<dyn Any + Send + Sync>;
type RegisterFn<T> = unsafe extern "C" fn(linker: &mut Linker<T>) -> Result<()>;

pub struct Plugin {
    id: &'static str,
    lib: Library,
}

pub trait PluginCtx {
    fn plugins(&self) -> &Arc<Vec<Plugin>>;
    fn plugin_state<T: 'static>(&self, plugin: &'static str) -> Option<&T>;
    fn plugin_state_mut<T: 'static>(&mut self, plugin: &'static str) -> Option<&mut T>;
}

impl Plugin {
    /// Loads a dynamic library as a plugin.
    ///
    /// # Safety
    ///
    /// Plugins must specify the correct type of the exported functions.
    pub unsafe fn new<P: AsRef<OsStr>>(filename: P) -> Result<Self> {
        let lib = Library::new(filename)?;
        let plugin_id: Symbol<PluginIDFn> = lib
            .get(b"plugin_id")
            .context("loading `plugin_id` export from plugin")?;
        Ok(Plugin {
            id: plugin_id(),
            lib,
        })
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn init(&self) -> Result<Box<dyn Any + Send + Sync>> {
        unsafe {
            let init: Symbol<InitFn> = self
                .lib
                .get(b"init")
                .context("loading `init` export from plugin")?;
            Ok(init())
        }
    }

    pub fn register<T>(&self, linker: &mut Linker<T>) -> Result<()> {
        unsafe {
            let register: Symbol<RegisterFn<T>> = self
                .lib
                .get(b"register")
                .context("loading `register` export from plugin")?;
            register(linker).context("calling register on plugin")
        }
    }

    // pub fn symbols(&self) -> Result<PluginSymbols> {
    //     let symbols = unsafe {
    //         PluginSymbols {
    //             register: self.lib.get(b"register")?,
    //         }
    //     };

    //     Ok(symbols)
    // }
}
