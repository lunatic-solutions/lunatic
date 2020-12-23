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
        Some(_) => quote! { state_wrapper.instance_environment().async_ },
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
        let state_wrapper = state.clone();
        let closure = move |#guest_signature_input| -> Result<#guest_signature_return, wasmtime::Trap> {
            #from_guest_input_transformations
            let result = Self::#method_name(state_wrapper.state(), #host_call_signature);
            let result = #maybe_async(result);
            Ok(#from_host_return_transformations(result)?)
        };
        linker.func(#namespace, #method_name_as_str, closure).unwrap();
    };
    Ok(result)
}
