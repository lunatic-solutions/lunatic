use  parity_wasm::elements::{serialize, deserialize_buffer, Module, Instruction, BlockType};
use parity_wasm::builder::{from_module, signature, import, ModuleBuilder};

pub fn insert_reduction_counting(wasm_buff: &[u8]) -> Vec<u8> {
    let wasm: Module = deserialize_buffer(wasm_buff).unwrap();
    // Determine the index of the to be inserted global variable.
    let reduction_counter = if let Some(global_section) = wasm.global_section() {
        global_section.entries().len() as u32
    } else {
        0
    };
    let builder = from_module(wasm);
    let builder = inject_global_reduction_counter(builder);
    let (yield_import, builder) = inject_yield_import(builder);
    let module = add_reduction_counter_to_all_functions(
        builder.build(),
        reduction_counter,
        yield_import
    );
    serialize(module).unwrap()
}

fn inject_global_reduction_counter(builder: ModuleBuilder) -> ModuleBuilder {
    builder.global()
        .value_type().i32()
        .init_expr(Instruction::I32Const(0))
        .mutable()
        .build()
}

fn inject_yield_import(mut builder: ModuleBuilder) -> (u32, ModuleBuilder) {
    let yield_sig = builder.push_signature(
		signature().build_sig()
    );
    let yield_import = builder.push_import(
        import()
        .module("lunatic")
		.field("yielder")
		.external().func(yield_sig)
		.build()
    );
    (yield_import, builder)
}

fn add_reduction_counter_to_all_functions(mut module: Module, reduction_counter: u32, yielder: u32) -> Module {
    let mut check_if_reduction_count_reached_10k_instructions = Vec::with_capacity(10);
    check_if_reduction_count_reached_10k_instructions.push(Instruction::GetGlobal(reduction_counter));
    check_if_reduction_count_reached_10k_instructions.push(Instruction::I32Const(1));
    check_if_reduction_count_reached_10k_instructions.push(Instruction::I32Add);
    check_if_reduction_count_reached_10k_instructions.push(Instruction::I32Const(10_000));
    check_if_reduction_count_reached_10k_instructions.push(Instruction::I32GtU);
    check_if_reduction_count_reached_10k_instructions.push(Instruction::If(BlockType::NoResult));
    check_if_reduction_count_reached_10k_instructions.push(Instruction::Call(yielder));
    check_if_reduction_count_reached_10k_instructions.push(Instruction::End);

    if let Some(code_section) = module.code_section_mut() {
        for func in code_section.bodies_mut() {
            let instructions = func.code_mut().elements_mut();
            instructions.splice(0..0, check_if_reduction_count_reached_10k_instructions.clone());
        }
    }
    module
}