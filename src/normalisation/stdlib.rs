use walrus::*;

/// Adds WASM functions required by the stdlib implementation:
/// * `lunatic_spawn_by_index(i32)`
///   - receives the index of the function (in the table) to be called indirectly.
pub fn patch(module: &mut Module) {
    if let Some(main_function_table) = module.tables.main_function_table().unwrap() {
        let mut builder = walrus::FunctionBuilder::new(
            &mut module.types,
            &[ValType::I32, ValType::I32, ValType::I32],
            &[],
        );
        let lunatic_spawn_by_index_type = module.types.add(&[ValType::I32, ValType::I32], &[]);
        // Create the index paramter
        let index = module.locals.add(ValType::I32);
        let argument1 = module.locals.add(ValType::I32);
        let argument2 = module.locals.add(ValType::I32);
        builder
            .func_body()
            .local_get(argument1)
            .local_get(argument2)
            .local_get(index)
            .call_indirect(lunatic_spawn_by_index_type, main_function_table);
        let function = builder.finish(vec![index, argument1, argument2], &mut module.funcs);
        module.exports.add("lunatic_spawn_by_index", function);
    }
}
