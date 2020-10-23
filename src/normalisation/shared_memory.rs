use walrus::*;

/// Finds memory with the index 0 and turns it into an import. Returns the initial memory size.
/// The `process::spawn()` function will crate a new memory and pass it into the module.
pub fn patch(module: &mut Module) -> u32 {
    if let Some(memory) = module.memories.iter_mut().next() {
        let memory_id = memory.id();
        let memory_import = module
            .imports
            .add("lunatic", "memory", ImportKind::Memory(memory_id));
        memory.shared = true;
        memory.import = Some(memory_import);
        memory.initial
    } else {
        0
    }
}
