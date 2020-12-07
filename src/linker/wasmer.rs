use wasmer::Engine;

pub struct LunaticLinker {}

impl LunaticLinker {
    /// Create a new LunaticLinker.
    pub fn new(
        engine: Engine,
        module: Module,
        yielder_ptr: usize,
        memory: MemoryChoice,
    ) -> Result<Self> {
        let store = Store::new(&engine);
        let mut linker = Linker::new(&store);

        let memory = match memory {
            MemoryChoice::Existing(memory) => memory,
            MemoryChoice::New(min_memory) => {
                let memory_ty = MemoryType::new(Limits::new(min_memory, None));
                Memory::new(&store, memory_ty)
            }
        };

        let environment =
            ProcessEnvironment::new(engine, module.clone(), memory.data_ptr(), yielder_ptr);

        linker.define("lunatic", "memory", memory)?;
        networking::api::add_to_linker(&mut linker, &environment)?;
        process::api::add_to_linker(&mut linker, &environment)?;
        channel::api::add_to_linker(&mut linker, &environment)?;
        wasi::api::add_to_linker(&mut linker, &environment)?;

        Ok(Self {
            store,
            linker,
            module,
            proc_env: environment,
        })
    }

    /// Create a new instance and set it up.
    /// This consumes the linker, as each of them is bound to one instance (environment).
    pub fn instance(self) -> Result<Instance> {
        let instance = self.linker.instantiate(&self.module)?;
        if let Some(externref_save) = instance.get_func("_lunatic_externref_save") {
            let stdin = ExternRef::new(std::io::stdin());
            assert_eq!(
                0,
                externref_save.call(&[Val::ExternRef(Some(stdin))])?[0].unwrap_i32()
            );
            let stdout = ExternRef::new(std::io::stdout());
            assert_eq!(
                1,
                externref_save.call(&[Val::ExternRef(Some(stdout))])?[0].unwrap_i32()
            );
            let stderr = ExternRef::new(std::io::stderr());
            assert_eq!(
                2,
                externref_save.call(&[Val::ExternRef(Some(stderr))])?[0].unwrap_i32()
            );
        }
        Ok(instance)
    }

    pub fn linker(&mut self) -> &mut Linker {
        &mut self.linker
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn proc_env(&mut self) -> &mut ProcessEnvironment {
        &mut self.proc_env
    }
}

/// Return a configured Wasmtime engine.
pub fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_threads(true);
    config.wasm_simd(true);
    config.wasm_reference_types(true);
    config.static_memory_guard_size(128 * 1024 * 1024); // 128 Mb
    Engine::new(&config)
}
