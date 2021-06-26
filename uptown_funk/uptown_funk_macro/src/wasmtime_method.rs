use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{ImplItemMethod, LitStr};

use crate::{
    attribute::SyncType,
    signature::{transform, Transform},
};

pub fn wrap(
    namespace: &LitStr,
    sync: SyncType,
    method: &ImplItemMethod,
) -> Result<TokenStream2, TokenStream> {
    let signature = &method.sig;
    let method_name = &signature.ident;
    let method_name_as_str = LitStr::new(&method_name.to_string(), method_name.span());

    // If it's an async function wrap it in an async block.
    let maybe_async = match signature.asyncness {
        Some(_) => quote! { cloned_executor.async_ },
        None => quote! { std::convert::identity },
    };

    let Transform {
        input_sig,
        output_sig,
        input_trans,
        call_args,
        output_trans,
    } = match transform(sync, &signature) {
        Ok(result) => result,
        Err(error) => return Err(error),
    };

    let lock_state = match sync {
        SyncType::None => quote! { let mut lstate = state.borrow_mut(); },
        SyncType::Mutex => {
            quote! { let mut lstate = state.lock().map_err(|e| ::wasmtime::Trap::new("State lock poisoned") )?; }
        }
    };

    let pass_state = match sync {
        SyncType::None => quote! { &mut lstate },
        SyncType::Mutex => quote! { &mut lstate },
    };

    let result = quote! {
        let state = api.clone();
        let cloned_executor = executor.clone();
        let closure = move |#input_sig| -> Result<#output_sig, wasmtime::Trap> {
            let cloned_executor = cloned_executor.as_ref();
            let memory = cloned_executor.memory();

            #input_trans;

            let output = {
                #lock_state;
                let result = Self::#method_name(#pass_state, #call_args);
                #maybe_async(result)
            };

            Ok(#output_trans(output)?)
        };

        for namespace in #namespace.split(',') {
            linker.func(namespace, #method_name_as_str, closure.clone()).unwrap();
        }
    };
    Ok(result)
}
