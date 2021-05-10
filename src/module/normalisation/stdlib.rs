use walrus::*;

/// Adds WASM functions required by the stdlib implementation:
/// * `lunatic_spawn_by_index(i32)`
///   - receives the index of the function (in the table) to be called indirectly.
pub fn patch(module: &mut Module) -> Result<()> {
    if let Some(main_function_table) = module.tables.main_function_table()? {
        let mut builder = walrus::FunctionBuilder::new(&mut module.types, &[ValType::I32], &[]);
        let lunatic_spawn_by_index_type = module.types.add(&[], &[]);
        // Create the index parameter
        let index = module.locals.add(ValType::I32);
        // invoke __wasm_call_ctors to properly setup environment
        // FIXME remove this when wasm adds proper environment initialisation
        match module.funcs.by_name("__wasm_call_ctors") {
            Some(ctors) => {
                builder.func_body().call(ctors);
            }
            // ignore if __wasm_call_ctors wasn't found
            None => log::info!("__wasm_call_ctors wasn't found."),
        };
        builder
            .func_body()
            .local_get(index)
            .call_indirect(lunatic_spawn_by_index_type, main_function_table);
        let function = builder.finish(vec![index], &mut module.funcs);
        module.exports.add("lunatic_spawn_by_index", function);
    }
    Ok(())
}
