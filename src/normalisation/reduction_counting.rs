use walrus::*;

// How many operations should happen before we yield
const REDUCTION_LIMIT: i32 = 10_000;

/// Modifies the WASM binary to add a `yield` call after `REDUCTION_LIMIT` of **operations** has
/// been reached. Function calls and loop iterations without calls are counted as **operations**.
/// The idea behind this is to not allow any WASM Instance to block a thread for too long.
///
/// To achieve this the following things are inserted into the WASM module:
/// * A global variable to hold the current count
/// * An import to the host provided `lunatic::yield` function
/// * Instructions on top of each function to check if we reached the `REDUCTION_LIMIT` and yield.
/// * Instructions on top of tight loops to check if we reached the `REDUCTION_LIMIT` and yield.
pub fn patch(module: &mut Module) {
    let counter = module
        .globals
        .add_local(ValType::I32, true, InitExpr::Value(ir::Value::I32(0)));
    let yield_type = module.types.add(&[], &[]);
    let yield_import = module.add_import_func("lunatic", "yield_", yield_type);

    // If a function is called inside a loop we can avoid inserting the reduction count inside of it, because all
    // function calls will also perform a reduction count. But this is not true for imported functions.
    // To make it easier to check if an imported function is called we keep a list of all of them around.
    let imported_functions: Vec<FunctionId> = module
        .imports
        .iter()
        .filter_map(|import| match import.kind {
            ImportKind::Function(function) => Some(function),
            _ => None,
        })
        .collect();

    for (_, function) in module.funcs.iter_local_mut() {
        patch_function(function, counter, yield_import.0, &imported_functions)
    }
}

fn patch_function(
    function: &mut LocalFunction,
    counter: GlobalId,
    yield_func: FunctionId,
    imported_functions: &Vec<FunctionId>,
) {
    let mut insertion_points = Vec::new();

    // Insert reduction counter at the top of every function
    let start = function.entry_block();
    insertion_points.push(start);

    // Check if there are tight loops
    let instr_seq = function.block(start);
    for (instr, _) in &instr_seq.instrs {
        match instr {
            ir::Instr::Loop(loop_) => {
                patch_sequence(
                    true,
                    loop_.seq,
                    function,
                    &mut insertion_points,
                    &imported_functions,
                );
            }
            ir::Instr::Block(block) => {
                patch_sequence(
                    false,
                    block.seq,
                    function,
                    &mut insertion_points,
                    &imported_functions,
                );
            }
            ir::Instr::IfElse(if_else) => {
                patch_sequence(
                    false,
                    if_else.consequent,
                    function,
                    &mut insertion_points,
                    &imported_functions,
                );
                patch_sequence(
                    false,
                    if_else.alternative,
                    function,
                    &mut insertion_points,
                    &imported_functions,
                );
            }
            _ => (),
        }
    }

    // Insert reduction counters in all pre-marked positions
    let builder = function.builder_mut();
    for insertion_point in insertion_points {
        let mut body = builder.instr_seq(insertion_point);
        body.block_at(0, None, |block| {
            insert_reduction_counter(block, counter, yield_func);
        });
    }
}

// Mark insertion points for reduction counter in loops that:
// * don't contain any other loops
// * don't contain calls to local functions
//
// Returns true if an insertion occurred in this block or any children, otherwise false.
fn patch_sequence(
    insert: bool,
    seq_id: ir::InstrSeqId,
    function: &LocalFunction,
    insertion_points: &mut Vec<ir::InstrSeqId>,
    imported_functions: &Vec<FunctionId>,
) -> bool {
    let mut child_inserts = false;
    let mut insert_reduction_counter = insert;
    let instr_seq = function.block(seq_id);

    for (instr, _) in &instr_seq.instrs {
        match instr {
            ir::Instr::Loop(loop_) => {
                patch_sequence(
                    true,
                    loop_.seq,
                    function,
                    insertion_points,
                    imported_functions,
                );
                insert_reduction_counter = false;
                child_inserts = true;
            }
            ir::Instr::Block(block) => {
                let inserted = patch_sequence(
                    false,
                    block.seq,
                    function,
                    insertion_points,
                    &imported_functions,
                );
                if inserted {
                    insert_reduction_counter = false;
                    child_inserts = true;
                }
            }
            ir::Instr::IfElse(if_else) => {
                let inserted_then = patch_sequence(
                    false,
                    if_else.consequent,
                    function,
                    insertion_points,
                    &imported_functions,
                );
                let inserted_else = patch_sequence(
                    false,
                    if_else.alternative,
                    function,
                    insertion_points,
                    &imported_functions,
                );
                if inserted_then && inserted_else {
                    insert_reduction_counter = false;
                    child_inserts = true;
                }
            }
            ir::Instr::Call(call) => {
                if !imported_functions.contains(&call.func) {
                    insert_reduction_counter = false;
                    child_inserts = true;
                }
            }
            ir::Instr::CallIndirect(_call_indirect) => {
                // On indirect calls we can't be sure that we are calling a local function.
                // The called function is not known at compile time.
            }
            _ => {}
        }
    }

    if insert_reduction_counter {
        insertion_points.push(instr_seq.id());
    }
    child_inserts
}

// Algorithm:
// 1. Increment the reduction counter global
// 2. Check if the global reached REDUCTION_LIMIT, if yes yield and reset reduction counter
fn insert_reduction_counter(
    block: &mut InstrSeqBuilder,
    counter: GlobalId,
    yield_func: FunctionId,
) {
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
                then.call(yield_func).i32_const(0).global_set(counter);
            },
            |_else| {},
        );
}
