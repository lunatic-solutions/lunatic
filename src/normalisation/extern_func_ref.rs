//! Lunatic tries to expose all resources (file descriptors, processes, sockets, ...) to WASM guest
//! code as externrefs. Usually, the signature of Lunatic imports returning resources looks something
//! like this:
//!
//!  (import "lunatic" "spawn" (func (;0;) (param i32 i64) (result externref)))
//!
//! Many programming languages (including Rust and C) don't have a way of dealing with externref types.
//! To work around this limitation, WASM code compiled from this languages defines their imports by
//! replacing externrefs with i32 args:
//!
//!  (import "lunatic" "spawn" (func (;0;) (param i32 i64) (result i32)))
//!
//! Obviously this type mismatch would be rejected by Wasmtime during instantiation. To make this
//! work and only provide one implementation (with Externrefs), Lunatic wraps the incompatible imports
//! in small wrapper functions:
//! * If the import returns an Externref, the wrapper saves it to a WASM table and returns the index
//!   in this table.
//! * If the import takes an Externref, the wrapper grabs the externref by provided index and passes
//!   it to the import.
//!
//! Lunatic exposes functions (`get_externref_free_slot` &` set_externref_free_slot`) that keep track
//! of free slots in the Externref table.
//!
//! Another transformation performed is the one on multi-value returns. If multi-value returns are
//! not supported by the WASM module it will usually use the last function parameters as pointers
//! to return the value through them. Lunatic's API only exports multi-value returns. To make them
//! work with languages only supporting a single return, a wrapper is needed to correctly map from:
//!
//!   (import "test" "test" (func (;0;) (param i64) (result i32 i32 externref)))
//!
//! to:
//!
//!   (import "test" "test" (func (;0;) (param i64 i32 i32) (result i32)))
//!
//! Notice how the first i32 is still returned, but the other 2 are now written to memory locations
//! passed as parameters 2 and 3. If an Externref is returned it's first transformed to an i32 that
//! represents its index in the Externref table and then the index is written to the memory location.

use crate::{
    linker::{engine, LunaticLinker},
    process::MemoryChoice,
};

use walrus::*;

#[derive(PartialEq, Clone)]
enum TransformationStep {
    ParamReturn(ValType),
    Externref,
    Funcref,
    Nop(ValType),
}

/// Instructions on how to transform imports to match the expected format.
struct Transformation {
    import_id: ImportId,
    function_id: FunctionId,
    params: Vec<TransformationStep>,
    params_return: Vec<TransformationStep>,
    results: Vec<TransformationStep>,
}

impl Transformation {
    // Create a transformation only if the signatures don't match.
    // `expected_type` refers to types provided by the Lunatic API.
    // `received_type` refers to types declared in the wasm file.
    pub fn from(
        import_id: ImportId,
        function_id: FunctionId,
        expected_type: wasmtime::FuncType,
        received_type: &Type,
    ) -> Option<Self> {
        // If there are more results than expected, rest of the results needs to be returned through params.
        let params_return: Vec<TransformationStep> =
            if expected_type.results().len() > received_type.results().len() {
                // Only if this conditions are met it's possible to do a correct transformation.
                assert_eq!(received_type.results().len(), 1);
                assert_eq!(
                    expected_type.params().len() + expected_type.results().len(),
                    received_type.params().len() + received_type.results().len()
                );
                // Skip first argument and transform others into parameter returns.
                expected_type
                    .results()
                    .into_iter()
                    .skip(1)
                    .map(|type_| {
                        TransformationStep::ParamReturn(match type_ {
                            wasmtime::ValType::I32 => ValType::I32,
                            wasmtime::ValType::I64 => ValType::I32,
                            wasmtime::ValType::F32 => ValType::I32,
                            wasmtime::ValType::F64 => ValType::I32,
                            wasmtime::ValType::V128 => ValType::I32,
                            wasmtime::ValType::ExternRef => ValType::Externref,
                            wasmtime::ValType::FuncRef => ValType::Funcref,
                        })
                    })
                    .collect()
            } else {
                Vec::with_capacity(0)
            };

        let params = Transformation::create_transformation_steps(
            expected_type.params(),
            received_type.params(),
        );
        let results = Transformation::create_transformation_steps(
            expected_type.results(),
            received_type.results(),
        );

        // If all transformation steps are Nop, no transformations are required.
        if params.iter().all(|step| match step {
            TransformationStep::Nop(_) => true,
            _ => false,
        }) && results.iter().all(|step| match step {
            TransformationStep::Nop(_) => true,
            _ => false,
        }) && params_return.len() == 0
        {
            None
        } else {
            Some(Self {
                import_id,
                function_id,
                params,
                params_return,
                results,
            })
        }
    }

    /// Calculates transformation steps to describe how to get from expected to received types.
    /// Only 3 scenarios per type are possible:
    /// * Expected type is Externref, but received on is I32 => TransformationStep::Externref
    /// * Expected type is Funcref, but received on is I32 => TransformationStep::Funcref
    /// * Expected and received types are the same => TransformationStep::Nop
    /// Other scenarios are currently not supported.
    fn create_transformation_steps<ExIter>(
        expected: ExIter,
        received: &[walrus::ValType],
    ) -> Vec<TransformationStep>
    where
        ExIter: ExactSizeIterator<Item = wasmtime::ValType>,
    {
        let mut result = Vec::with_capacity(expected.len());
        expected
            .zip(received.iter())
            .for_each(|(ex_type, rec_type)| {
                if (ex_type == wasmtime::ValType::I32 && rec_type.eq(&walrus::ValType::I32))
                    || (ex_type == wasmtime::ValType::I64 && rec_type.eq(&walrus::ValType::I64))
                    || (ex_type == wasmtime::ValType::F32 && rec_type.eq(&walrus::ValType::F32))
                    || (ex_type == wasmtime::ValType::F64 && rec_type.eq(&walrus::ValType::F64))
                    || (ex_type == wasmtime::ValType::V128 && rec_type.eq(&walrus::ValType::V128))
                    || (ex_type == wasmtime::ValType::ExternRef
                        && rec_type.eq(&walrus::ValType::Externref))
                    || (ex_type == wasmtime::ValType::FuncRef
                        && rec_type.eq(&walrus::ValType::Funcref))
                {
                    // Nothing to do
                    result.push(TransformationStep::Nop(rec_type.clone()));
                } else {
                    // Externref -> i32 transformation
                    if ex_type.eq(&wasmtime::ValType::ExternRef)
                        && rec_type.eq(&walrus::ValType::I32)
                    {
                        result.push(TransformationStep::Externref);
                    // Externref -> i32 transformation
                    } else if ex_type.eq(&wasmtime::ValType::FuncRef)
                        && rec_type.eq(&walrus::ValType::I32)
                    {
                        result.push(TransformationStep::Funcref);
                    } else {
                        unreachable!("Unsupported transformation: {} => {}", ex_type, rec_type);
                    }
                }
            });
        result
    }

    // Return type of import function after the transformation.
    // (params, results)
    pub fn import_type(&self) -> (Vec<ValType>, Vec<ValType>) {
        let import_type_resolver =
            |transformation_step: &TransformationStep| match transformation_step {
                TransformationStep::Nop(val_type) => val_type.clone(),
                TransformationStep::Externref => ValType::Externref,
                TransformationStep::Funcref => ValType::Funcref,
                TransformationStep::ParamReturn(val_type) => val_type.clone(),
            };

        let params = self.params.iter().map(import_type_resolver).collect();
        let mut results_extended = self.results.clone();
        results_extended.extend(self.params_return.clone());
        let results = results_extended.iter().map(import_type_resolver).collect();
        (params, results)
    }

    // Return type of import wrapper function after the transformation.
    // (params, results)
    pub fn wrapper_type(&self) -> (Vec<ValType>, Vec<ValType>) {
        let import_type_resolver =
            |transformation_step: &TransformationStep| match transformation_step {
                TransformationStep::Nop(val_type) => val_type.clone(),
                _ => ValType::I32,
            };

        let mut params_extended = self.params.clone();
        params_extended.extend(self.params_return.clone());
        let params = params_extended.iter().map(import_type_resolver).collect();
        let results = self.results.iter().map(import_type_resolver).collect();
        (params, results)
    }
}

pub fn patch(module: &mut Module) {
    // Create a LunaticLinker for an empty module just to extract all the signatures from generated imports.
    // The data passed to LunaticLinker::new in this case doesn't have any impotance and is mocked.
    let engine = engine();
    let temp_module = wasmtime::Module::new(&engine, "(module)").unwrap();
    let mut lunatic_linker =
        LunaticLinker::new(engine, temp_module, 0, MemoryChoice::New(0)).unwrap();
    let wasmtime_linker = lunatic_linker.linker();

    // Collect all functions to be transformed by comparing expected and given signatures.
    let mut functions_to_transfrom = Vec::new();
    module.imports.iter().for_each(|import| match import.kind {
        ImportKind::Function(function_id) => {
            wasmtime_linker
                .get_by_name(&import.module, &import.name)
                .for_each(|ex| match ex {
                    wasmtime::Extern::Func(function) => {
                        let expected_type = function.ty();
                        let function = module.funcs.get(function_id);
                        let received_type = module.types.get(function.ty());
                        if let Some(transformation) = Transformation::from(
                            import.id(),
                            function_id,
                            expected_type,
                            received_type,
                        ) {
                            functions_to_transfrom.push(transformation);
                        }
                    }
                    _ => panic!("Import defined as function not inside lunatic"),
                });
        }
        _ => (),
    });

    if functions_to_transfrom.len() > 0 {
        let (resource_table, save_externref) = add_externref_save_drop_extend3(module);
        for transformation in functions_to_transfrom.into_iter() {
            // Declare new import with externref/funcref types
            let import = module.imports.get(transformation.import_id);
            let import_module = import.module.clone();
            let import_name = import.name.clone();
            let (import_params, import_return) = transformation.import_type();
            let import_type = {
                module
                    .types
                    .add(import_params.as_slice(), import_return.as_slice())
            };
            let (import, _id) = module.add_import_func(&import_module, &import_name, import_type);

            // Delete previous import
            module.imports.delete(transformation.import_id);

            // Create wrapper function using new import
            let (wrapper_params, wrapper_return) = transformation.wrapper_type();

            let mut wrapper_builder = walrus::FunctionBuilder::new(
                &mut module.types,
                wrapper_params.as_slice(),
                wrapper_return.as_slice(),
            );
            let wrapper_arguments: Vec<LocalId> = wrapper_params
                .iter()
                .map(|val_type| module.locals.add(val_type.clone()))
                .collect();

            let mut instructions = wrapper_builder.func_body();
            let main_function_table = module.tables.main_function_table().unwrap();
            // If we are passing externrefs to the import, grab them first from the table.
            transformation
                .params
                .iter()
                .enumerate()
                .for_each(|(i, step)| match step {
                    TransformationStep::Nop(_) => {
                        instructions.local_get(wrapper_arguments[i]);
                    }
                    TransformationStep::Externref => {
                        instructions
                            .local_get(wrapper_arguments[i])
                            .table_get(resource_table);
                    }
                    TransformationStep::Funcref => match main_function_table {
                        Some(table_id) => {
                            instructions
                                .local_get(wrapper_arguments[i])
                                .table_get(table_id);
                        }
                        None => panic!("Can't take a Funcref without a main function table"),
                    },
                    TransformationStep::ParamReturn(_) => panic!("Can't appear in params"),
                });

            // Call the wrapped import
            instructions.call(import);

            // If we are returning values through params, write them to the correct memory location.
            let offset = transformation.params.len();
            transformation
                .params_return
                .iter()
                .enumerate()
                .rev()
                .for_each(|(i, step)| match step {
                    TransformationStep::ParamReturn(type_) => {
                        let main_memory_id = module.memories.iter().next().unwrap().id();
                        let i = i + offset;
                        match type_ {
                            ValType::Funcref => unimplemented!(""),
                            ValType::Externref => {
                                let temp = module.locals.add(ValType::I32);
                                instructions
                                    .call(save_externref)
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::I32 { atomic: false },
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                            ValType::I32 => {
                                let temp = module.locals.add(ValType::I32);
                                instructions
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::I32 { atomic: false },
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                            ValType::I64 => {
                                let temp = module.locals.add(ValType::I64);
                                instructions
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::I64 { atomic: false },
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                            ValType::F32 => {
                                let temp = module.locals.add(ValType::F32);
                                instructions
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::F32,
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                            ValType::F64 => {
                                let temp = module.locals.add(ValType::F64);
                                instructions
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::F64,
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                            ValType::V128 => {
                                let temp = module.locals.add(ValType::V128);
                                instructions
                                    .local_set(temp)
                                    .local_get(wrapper_arguments[i])
                                    .local_get(temp)
                                    .store(
                                        main_memory_id,
                                        ir::StoreKind::V128,
                                        ir::MemArg {
                                            align: 1,
                                            offset: 0,
                                        },
                                    );
                            }
                        }
                    }
                    _ => unreachable!("Can't contain any other values"),
                });

            // If we are returning externrefs, save them first in the resource table and return index to them.
            // TODO: Only one return supported without returning values as params at this time.
            assert!(transformation.results.len() <= 1);
            transformation.results.iter().for_each(|step| match step {
                TransformationStep::Nop(_) => {}
                TransformationStep::Externref => {
                    instructions.call(save_externref);
                }
                TransformationStep::Funcref => unimplemented!("TODO: Implement this!"),
                TransformationStep::ParamReturn(_) => panic!("Can't appear in results"),
            });

            let import_function = module.funcs.get_mut(transformation.function_id);
            replace_import_with_local_function(import_function, wrapper_builder, wrapper_arguments);
        }
    }
}

/// Replaces all calls to the imported function with a local one **in place**.
/// This is currently not supported in Walrus, so an unsafe transmute is used to perfrom the operation.
fn replace_import_with_local_function(
    import_function: &mut Function,
    builder: FunctionBuilder,
    args: Vec<LocalId>,
) {
    // To swap out an import an unsafe trick is used.
    // https://github.com/rustwasm/walrus/issues/186
    struct UnsafeLocalFunction {
        _builder: FunctionBuilder,
        _args: Vec<LocalId>,
    }

    let unsafe_local_wrapper = UnsafeLocalFunction {
        _builder: builder,
        _args: args,
    };

    unsafe {
        // Old import is "in place" replaced by new import wrapper.
        import_function.kind = FunctionKind::Local(std::mem::transmute(unsafe_local_wrapper));
    }
}

/// Adds to the module:
/// * _lunatic_externref_save(externref) -> index
///   Preserves the externref in the externref table and returns the index inside the table.
///
/// Replaces import of `lunatic::drop_externref` with a local function that drops the externref.
fn add_externref_save_drop_extend3(module: &mut Module) -> (TableId, FunctionId) {
    let resource_table = module.tables.add_local(4, None, ValType::Externref);
    module
        .exports
        .add("__lunatic_externref_resource_table", resource_table);

    // _lunatic_externref_save(externref) -> index
    let get_externref_free_slot_type = module.types.add(&[], &[ValType::I32]);
    let (get_externref_free_slot, _) = module.add_import_func(
        "lunatic",
        "get_externref_free_slot",
        get_externref_free_slot_type,
    );

    let mut save_builder =
        walrus::FunctionBuilder::new(&mut module.types, &[ValType::Externref], &[ValType::I32]);
    let externref = module.locals.add(ValType::Externref);
    let free_slot = module.locals.add(ValType::I32);
    save_builder
        .func_body()
        .call(get_externref_free_slot)
        .local_tee(free_slot)
        .table_size(resource_table)
        .binop(ir::BinaryOp::I32Eq)
        .if_else(
            Some(ValType::I32),
            |then| {
                // If we don't have nough space for this index, double the table first.
                then.ref_null(ValType::Externref)
                    .table_size(resource_table)
                    .table_grow(resource_table);
            },
            |else_| {
                else_.local_get(free_slot);
            },
        )
        .local_get(externref)
        .table_set(resource_table)
        .local_get(free_slot);
    let save_externref = save_builder.finish(vec![externref], &mut module.funcs);
    module
        .exports
        .add("_lunatic_externref_save", save_externref);

    // _lunatic_externref_drop(index)
    if let Some(externref_drop_import_id) = module.imports.find("lunatic", "drop_externref") {
        let externref_drop_import = module.imports.get(externref_drop_import_id);
        let externref_drop_func_id = match externref_drop_import.kind {
            ImportKind::Function(function) => function,
            _ => panic!("lunatic::externref_drop must be a function"),
        };

        let set_externref_free_slot_type = module.types.add(&[ValType::I32], &[]);
        let (set_externref_free_slot, _) = module.add_import_func(
            "lunatic",
            "set_externref_free_slot",
            set_externref_free_slot_type,
        );

        let mut drop_builder =
            walrus::FunctionBuilder::new(&mut module.types, &[ValType::I32], &[]);
        let free_slot = module.locals.add(ValType::I32);
        drop_builder
            .func_body()
            .local_get(free_slot)
            .ref_null(ValType::Externref)
            .table_set(resource_table)
            .local_get(free_slot)
            .call(set_externref_free_slot);

        let externref_drop_import = module.funcs.get_mut(externref_drop_func_id);
        replace_import_with_local_function(externref_drop_import, drop_builder, vec![free_slot]);

        module
            .exports
            .add("_lunatic_externref_drop", externref_drop_func_id);

        // Delete the import once it's replaced
        module.imports.delete(externref_drop_import_id);
    };

    (resource_table, save_externref)
}
