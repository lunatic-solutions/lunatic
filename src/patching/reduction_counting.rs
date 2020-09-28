use walrus::*;

// How many operations should happen before we yield
const REDUCTION_LIMIT: i32 = 100_000;

/// Modifies the WASM binary to add a `yield` import call after `REDUCTION_LIMIT` of **operations** is reached.
/// Currently only function calls are counted as **operations**.
/// TODO: If there is a tight loop without function calls, this becomes an issue. We need to check for this case.
/// The idea behind this is to not allow any WASM Instance to block a thread in an async environment for too long.
/// To achive this the following things are inserted into the WASM module:
/// * Global variable to hold the current count
/// * An import to the host provided `yield` function
/// * Instructions on top of each function to check if we reached the `REDUCTION_LIMIT` and yield
pub fn patch(module: &mut Module) {
    let counter = module.globals.add_local(ValType::I32, true, InitExpr::Value(ir::Value::I32(0)));
    let yield_type = module.types.add(&[], &[]);
    let yield_import = module.add_import_func("lunatic", "yield", yield_type);

    for function in module.funcs.iter_mut() {
        match &mut function.kind {
            FunctionKind::Local(function) => patch_function(function, counter, yield_import.0),
            _ => continue
        }
    }
}

fn patch_function(function: &mut LocalFunction, counter: GlobalId, yield_func: FunctionId) {
    let builder = function.builder_mut();
    let mut body = builder.func_body();
    body.block_at(0, None, |block| {
        // Algorithm:
        // 1. Increment the reduction counter global
        // 2. Check if the global reached 100_000, if yes yiled and reset reduction counter
        block
            .global_get(counter)
            .i32_const(1)
            .binop(ir::BinaryOp::I32Add)
            .global_set(counter)
            .global_get(counter)
            .i32_const(REDUCTION_LIMIT)
            .binop(ir::BinaryOp::I32GtS)
            .if_else(
                None,
                |then| {
                    then
                        .call(yield_func)
                        .i32_const(0)
                        .global_set(counter);
                },
                |_else| {},
            );
    });

}