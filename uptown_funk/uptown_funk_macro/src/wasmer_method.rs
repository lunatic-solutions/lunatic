use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{ImplItemMethod, LitStr};

use crate::signature::transform;

pub fn wrap(namespace: &LitStr, method: &ImplItemMethod) -> Result<TokenStream2, TokenStream> {
    let signature = &method.sig;
    let method_name = &signature.ident;
    let method_name_as_str = LitStr::new(&method_name.to_string(), method_name.span());

    // If it's an async function wrap it in an async block.
    let maybe_async = match signature.asyncness {
        Some(_) => quote! { state_wrapper.executor().async_ },
        None => quote! { std::convert::identity },
    };

    let (
        guest_signature_input,
        guest_signature_return,
        from_guest_input_transformations,
        host_call_signature,
        from_host_return_transformations,
    ) = match transform(&signature) {
        Ok(result) => result,
        Err(error) => return Err(error),
    };

    let result = quote! {
        let closure = |state_wrapper: &uptown_funk::StateWrapper<Self, E>, #guest_signature_input|
         -> #guest_signature_return {
            // Wasmer host functions can only return simple types and we must manually raise a trap.
            match (|| -> Result<#guest_signature_return, uptown_funk::Trap> {
                let memory = state_wrapper.memory();
                #from_guest_input_transformations
                let result = {
                    let mut borrow = state_wrapper.borrow_state_mut();
                    let result = Self::#method_name(&mut borrow, #host_call_signature);
                    #maybe_async(result)
                };
                Ok(#from_host_return_transformations(result)?)
            })() {
                Ok(result) => result,
                Err(trap) => unsafe { wasmer::raise_user_trap(Box::new(trap.with_data(state_wrapper.clone()))) }
            }
        };

        let func = wasmer::Function::new_native_with_env(store, state.clone(), closure);
        for namespace in #namespace.split(",") {
            wasmer_linker.add(namespace, #method_name_as_str, wasmer::Exportable::to_export(&func));
        }
    };
    Ok(result)
}
