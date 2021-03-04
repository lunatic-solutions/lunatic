use walrus::*;

/// Finds memory with the index 0 and turns it into an import.
/// Returns the initial and maximum memory sizes.
pub fn patch(module: &mut Module) -> (u32, Option<u32>) {
    if let Some(memory) = module.memories.iter_mut().next() {
        let memory_id = memory.id();
        let memory_import = module
            .imports
            .add("lunatic", "memory", ImportKind::Memory(memory_id));
        memory.shared = false;
        memory.import = Some(memory_import);
        (memory.initial, memory.maximum)
    } else {
        (0, None)
    }
}
