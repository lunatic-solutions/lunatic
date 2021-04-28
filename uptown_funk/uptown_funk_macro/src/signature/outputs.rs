use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Ident, Index, Type, TypePath};

use crate::attribute::SyncType;

use super::return_error;

/// Takes the return arguments part of the host function's signature and returns wrappers around higher
/// level types to make them compatible with WASM guest functions, according to WASI conventions.
///
/// WASI convetion mandates that there can only be at most one return value. All other return values are
/// returned throug pointers of input arguments. Because of this the following transformation modifies the
/// input arguments if there is more than one return value.
///
/// There are 5 parts to this transformation (the return values):
/// 1. Input signature of the wasm guest function with the spilled over return values.
/// 2. Return signature of the wasm guest function with at most one value.
/// 3. Transformation from guest to host input arguments
/// 4. Signature of host call
/// 5. Transformation from host to guest return arguments

/// The following rules are followed when doing the transformation:
/// 1. One return value of type **i32, i64, f32 and f64** (WASM guest compatible types) is returned as is.
/// 2. If there **are multiple return values of type i32, i64, f32 and f64** (WASM guest compatible types),
///    the first one is returned as is, but the rest are returned through pinters of input arguments.
/// 3. **Custom types** need to implement uptown_funk::ToWasm and are transformed to an **i32** wasm type,
///    then they also follow rules 1 and 2.

pub fn transform(
    sync: SyncType,
    return_type: &Type,
) -> Result<
    (
        TokenStream2,
        TokenStream2,
        TokenStream2,
        TokenStream2,
        TokenStream2,
    ),
    TokenStream,
> {
    match return_type {
        Type::Path(type_path) => {
            let (return_argument, host_to_guest_transformation) = first_output(sync, type_path)?;
            Ok((
                quote! {},
                return_argument,
                quote! {},
                quote! {},
                host_to_guest_transformation,
            ))
        }
        Type::Tuple(type_tuple) => {
            // Returning multiple values as a tuple
            let mut return_types = type_tuple.elems.iter();

            // let input_arguments = Vec::new();
            let mut return_argument = quote! {};
            let mut return_argument_transformation = quote! {};
            // First type is returned as value.
            if let Some(first_type) = return_types.next() {
                match first_type {
                    Type::Path(type_path) => {
                        let (return_argument_first, host_to_guest_transformation) =
                            first_output(sync, type_path)?;
                        return_argument = return_argument_first;
                        return_argument_transformation = host_to_guest_transformation;
                    }
                    _ => return Err(return_error(first_type)),
                };
            }

            let mut input_argument_extensions = Vec::new();
            let mut return_argument_to_input_transformation = Vec::new();
            // Other values are returned through argument pointers.
            for (i, return_type) in return_types.enumerate() {
                let i = i + 1;
                let index = Index::from(i);
                let varname = format!("return_argument_as_ptr_{}", i);
                let varname = Ident::new(&varname, return_type.span());

                input_argument_extensions.push(quote! { #varname: u32 });

               let (to_wasm_generic_type, to_wasm_state_prepare, to_wasm_state_param) = to_wasm_tokens(sync);
 
                match return_type {
                    Type::Path(type_path) => {
                        if let Some(ident) = type_path.path.get_ident() {
                            if ident == "u32"
                                || ident == "i32"
                                || ident == "u64"
                                || ident == "i64"
                                || ident == "f32"
                                || ident == "f64"
                            {
                                // Simple type
                                return_argument_to_input_transformation.push( quote! {
                                    let result_ptr = {
                                        let memory = memory.as_mut_slice();
                                        let memory: &mut [#ident] = unsafe { std::mem::transmute(memory) };
                                        memory.get_mut(#varname as usize / std::mem::size_of::<#ident>())
                                    };
                                    let result_ptr = uptown_funk::Trap::try_option(result_ptr)?;
                                    *result_ptr = result.#index;
                                });
                            } else {
                                // Custom type
                                return_argument_to_input_transformation.push(quote! {
                                    let result_ptr = {
                                        let memory = memory.as_mut_slice();
                                        let memory: &mut [<#type_path as uptown_funk::ToWasm<#to_wasm_generic_type>>::To]
                                            = unsafe { std::mem::transmute(memory) };
                                        memory.get_mut(
                                            #varname as usize / std::mem::size_of::<<#type_path as uptown_funk::ToWasm<#to_wasm_generic_type>>::To>())
                                    };
                                    let result_ptr = uptown_funk::Trap::try_option(result_ptr)?;
                                    #to_wasm_state_prepare;
                                    let result_ = <#type_path as uptown_funk::ToWasm<#to_wasm_generic_type>>::to(
                                        #to_wasm_state_param,
                                        cloned_executor.as_ref(),
                                        result.#index
                                    )?;
                                    *result_ptr = result_;
                                });
                            }
                        } else {
                            return Err(return_error(type_path));
                        }
                    }
                    _ => return Err(return_error(return_type)),
                };
            }

            let host_to_guest_transformation = quote! {
                |result: #type_tuple| -> Result<#return_argument, uptown_funk::Trap> {
                    #(#return_argument_to_input_transformation)*
                    #return_argument_transformation(result.0)
                }
            };

            let input_arguments = quote! { #(#input_argument_extensions),* };

            return Ok((
                input_arguments,
                return_argument,
                quote! {},
                quote! {},
                host_to_guest_transformation,
            ));
        }
        _ => Err(return_error(return_type)),
    }
}

// First output is always returned as a regular return value.
fn first_output(sync: SyncType, type_path: &TypePath) -> Result<(TokenStream2, TokenStream2), TokenStream> {
    if let Some(ident) = type_path.path.get_ident() {
        if ident == "u32"
            || ident == "i32"
            || ident == "u64"
            || ident == "i64"
            || ident == "f32"
            || ident == "f64"
        {
            // Returning simple type
            let return_argument = quote! { #ident };
            let host_to_guest_transformation =
                // Identity
                quote! { |a: #ident| -> Result<#ident, uptown_funk::Trap> {Ok(a)} };
            return Ok((return_argument, host_to_guest_transformation));
        } else {
            // Returning CustomType
            let (to_wasm_generic_type, to_wasm_state_prepare, to_wasm_state_param) = to_wasm_tokens(sync);
            let return_argument = quote! { <#ident as uptown_funk::ToWasm<#to_wasm_generic_type>>::To };
            let host_to_guest_transformation = quote! {
                | output: #ident | -> Result<<#ident as uptown_funk::ToWasm<#to_wasm_generic_type>>::To, uptown_funk::Trap> {
                    #to_wasm_state_prepare;
                    <#ident as uptown_funk::ToWasm<#to_wasm_generic_type>>::to(
                        #to_wasm_state_param,
                        cloned_executor.as_ref(),
                        output
                    )
                }
            };
            return Ok((return_argument, host_to_guest_transformation));
        }
    }
    Err(return_error(type_path))
}


fn to_wasm_tokens(sync: SyncType) -> (TokenStream2, TokenStream2, TokenStream2) {
    let to_wasm_generic_type = match sync {
        SyncType::None => quote! { &mut Self::Wrap },
        SyncType::Mutex => quote! { &Self::Wrap }
    };

    let to_wasm_state_prepare = match sync {
        SyncType::None => quote! { let mut pstate = state.borrow_mut() },
        SyncType::Mutex => quote! { }
    };

    let to_wasm_state_param = match sync {
        SyncType::None => quote! { &mut pstate },
        SyncType::Mutex => quote! { &state }
    };

    (to_wasm_generic_type, to_wasm_state_prepare, to_wasm_state_param)
}