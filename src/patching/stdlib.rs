use walrus::*;

/// Adds WASM functions required by the stdlib implementation:
/// * `lunatic_spawn_by_index(i32)`
///   - receives the index of the function (in the table) to be called indirectly.
pub fn patch(module: &mut Module) {
    if let Some(main_function_table) = module.tables.main_function_table().unwrap() {
        let mut builder =
            walrus::FunctionBuilder::new(&mut module.types, &[ValType::I32, ValType::I64], &[]);
        let lunatic_spawn_by_index_type = module.types.add(&[ValType::I64], &[]);
        // Create the index paramter
        let index = module.locals.add(ValType::I32);
        let argument = module.locals.add(ValType::I64);
        builder
            .func_body()
            .local_get(argument)
            .local_get(index)
            .call_indirect(lunatic_spawn_by_index_type, main_function_table);
        let function = builder.finish(vec![index, argument], &mut module.funcs);
        module.exports.add("lunatic_spawn_by_index", function);
    }
}
