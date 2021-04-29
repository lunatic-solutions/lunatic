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
            quote! { let mut lstate = state.lock().map_err(|_| uptown_funk::Trap::new(""))?; }
        }
    };

    let pass_state = match sync {
        SyncType::None => quote! { &mut lstate },
        SyncType::Mutex => quote! { &mut lstate },
    };

    let state_wrapper_state_type = match sync {
        SyncType::None => quote! { ::std::rc::Rc<::std::cell::RefCell<Self::Wrap>> },
        SyncType::Mutex => quote! { Self::Wrap },
    };

    let result = quote! {
        let closure = |env: &uptown_funk::StateWrapper<#state_wrapper_state_type, E>, #input_sig|
         -> #output_sig {
            // Wasmer host functions can only return simple types and we must manually raise a trap.
            match (|| -> Result<#output_sig, uptown_funk::Trap> {
                let memory = env.memory();
                let cloned_executor = env.executor().clone();
                let state = env.state.clone();

                #input_trans;

                let output = {
                    #lock_state;
                    let result = Self::#method_name(#pass_state, #call_args);
                    #maybe_async(result)
                };

                Ok(#output_trans(output)?)
            })() {
                Ok(result) => result,
                Err(trap) => unsafe { wasmer::raise_user_trap(Box::new(trap)) }
            }
        };

        let func = wasmer::Function::new_native_with_env(store, state_wrapper.clone(), closure);
        for namespace in #namespace.split(",") {
            wasmer_linker.add(namespace, #method_name_as_str, wasmer::Exportable::to_export(&func));
        }
    };
    Ok(result)
}
