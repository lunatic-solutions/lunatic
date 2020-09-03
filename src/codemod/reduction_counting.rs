use  parity_wasm::elements::{serialize, deserialize_buffer, Module, Instruction, BlockType, External, Internal};
use parity_wasm::builder::{from_module, signature, import, ModuleBuilder};

// How many function calls should happen before we yield
const REDUCTION_LIMIT: i32 = 100_000;

/// Modifies the WASM binary to add a `yield` function call after `REDUCTION_LIMIT` of **operations** is reached.
/// Currently only function calls are counted as **operations**.
/// TODO: If there is a tight loop without function calls, this becomes an issue. We need to check for this case.
/// The idea behind this is to not allow any WASM Instance to block a thread in an async environment for too long.
/// To achive this the following things are inserted into the WASM module:
/// * Global variable to hold the current count
/// * An import to the host provided `yield` function
/// * Instructions on top of each function to check if we reached the `REDUCTION_LIMIT` and yield
///
/// Some of the insertions can break the existing code. E.g. if we insert a new imported function, all the indexes
/// in **functino calls** and **exports** become invalid and need to be updated.
pub fn insert_reduction_counting(wasm_buff: &[u8]) -> Vec<u8> {
    let wasm: Module = deserialize_buffer(wasm_buff).unwrap();
    // Determine the index of the to be inserted global variable.
    // This was fixed in: https://github.com/paritytech/parity-wasm/pull/291
    // Once the release is out, update.
    let reduction_counter = if let Some(global_section) = wasm.global_section() {
        global_section.entries().len() as u32
    } else {
        0
    };

    // Determin how many function imports there are before we inject a new one so we can offset
    // all function calls & exports by the correct value.
    let yield_import = if let Some(import_section) = &wasm.import_section() {
        import_section.entries().iter().fold(0, |acc, import|
            match import.external() {
                External::Function(_) => acc + 1,
                _ => acc
            }
        )
    } else {
        0
    };

    let builder = from_module(wasm);
    let builder = inject_global_reduction_counter(builder);

    let builder = inject_yield_import(builder);
    let module = inject_reduction_counter_to_all_functions(
        builder.build(),
        reduction_counter,
        yield_import
    );

    let module = update_export_indexes(module, yield_import);
    serialize(module).unwrap()
}

/// Injects a global mutable value initalised with 0.
fn inject_global_reduction_counter(builder: ModuleBuilder) -> ModuleBuilder {
    builder.global()
        .value_type().i32()
        .init_expr(Instruction::I32Const(0))
        .mutable()
        .build()
}

/// Injects a function import.
fn inject_yield_import(mut builder: ModuleBuilder) -> ModuleBuilder {
    let yield_sig = builder.push_signature(
		signature().build_sig()
    );
    builder.push_import(
        import()
        .module("lunatic")
		.field("yield")
		.external().func(yield_sig)
		.build()
    );
    builder
}

/// Inject code for reduction counting on top of each funciton and update function calls to the right offset.
fn inject_reduction_counter_to_all_functions(mut module: Module, reduction_counter: u32, yielder: u32) -> Module {
    // Algorithm:
    // 1. Increment the reduction counter global
    // 2. Check if the global reached 10_000, if yes yiled and reset reduction counter
    let mut injected_instructions = Vec::with_capacity(12);
    injected_instructions.push(Instruction::GetGlobal(reduction_counter));
    injected_instructions.push(Instruction::I32Const(1));
    injected_instructions.push(Instruction::I32Add);
    injected_instructions.push(Instruction::SetGlobal(reduction_counter));
    injected_instructions.push(Instruction::GetGlobal(reduction_counter));
    injected_instructions.push(Instruction::I32Const(REDUCTION_LIMIT));
    injected_instructions.push(Instruction::I32GtS);
    injected_instructions.push(Instruction::If(BlockType::NoResult));
    injected_instructions.push(Instruction::Call(yielder));
    injected_instructions.push(Instruction::I32Const(0));
    injected_instructions.push(Instruction::SetGlobal(reduction_counter));
    injected_instructions.push(Instruction::End);

    if let Some(code_section) = module.code_section_mut() {
        for func in code_section.bodies_mut() {
            let instructions = func.code().elements();
            // Offset all call instructinos by 1 if they point to an index after the yielder.
            let mut instructions: Vec<Instruction> = instructions.iter().map(|instruction| match instruction {
                Instruction::Call(index) =>
                    if *index >= yielder {
                        Instruction::Call(index + 1)
                    } else {
                        instruction.clone()
                    },
                _ => instruction.clone()
            }).collect();

            // Add a reduction counter check before any other code in the function.
            // We need to clone every time, to not polute injected_instructions for the next function.
            let mut new_function = injected_instructions.clone();
            new_function.append(&mut instructions);

            *func.code_mut().elements_mut() = new_function;
        }
    }
    module
}

/// Updated exported functions with the right offset, becuase we added one more imported function.
fn update_export_indexes(mut module: Module, yielder: u32) -> Module {
    if let Some(export_section) = module.export_section_mut() {
        for export in export_section.entries_mut() {
            match export.internal() {
                Internal::Function(index) =>
                    if *index >= yielder {
                        *export.internal_mut() = Internal::Function(*index + 1);
                    },
                _ => ()
            }
        }
    }
    module
}