use walrus::*;

/// Finds memory with the index 0 and turns it into an import.
/// Returns the initial and maximum memory sizes.
pub fn patch(module: &mut Module) -> (u32, Option<u32>) {
    if let Some(memory) = module.memories.iter_mut().next() {
        if let Some(_import) = memory.import {
            // If existing memory is an imported one don't change it.
            (memory.initial, memory.maximum)
        } else {
            // Create memory import
            let memory_import =
                module
                    .imports
                    .add("lunatic", "memory", ImportKind::Memory(memory.id()));
            // Change existing memory to import
            memory.shared = false;
            memory.import = Some(memory_import);

            (memory.initial, memory.maximum)
        }
    } else {
        (0, None)
    }
}
