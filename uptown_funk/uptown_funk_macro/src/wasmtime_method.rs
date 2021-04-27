use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{ImplItemMethod, LitStr};

use crate::signature::{Transform, transform};

pub fn wrap(namespace: &LitStr, method: &ImplItemMethod) -> Result<TokenStream2, TokenStream> {
    let signature = &method.sig;
    let method_name = &signature.ident;
    let method_name_as_str = LitStr::new(&method_name.to_string(), method_name.span());

    // If it's an async function wrap it in an async block.
    let maybe_async = match signature.asyncness {
        Some(_) => quote! { cloned_executor.async_ },
        None => quote! { std::convert::identity },
    };

    let Transform {
        input_sig, output_sig, input_trans, call_args, output_trans,
    } = match transform(&signature) {
        Ok(result) => result,
        Err(error) => return Err(error),
    };

    let result = quote! {
        let state = api.clone();
        let cloned_executor = executor.clone();
        let closure = move |#input_sig| -> Result<#output_sig, wasmtime::Trap> {
            let memory = cloned_executor.memory();

            #input_trans;

            let output = {
                // TODO assumes Mutex
                let state = &mut state.lock().unwrap();
                let result = Self::#method_name(state, #call_args);
                #maybe_async(result)
            };

            Ok(#output_trans(output)?)
        };

        for namespace in #namespace.split(",") {
            linker.func(namespace, #method_name_as_str, closure.clone()).unwrap();
        }
    };
    Ok(result)
}
