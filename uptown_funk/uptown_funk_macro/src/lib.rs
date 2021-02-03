mod attribute;
mod signature;
mod state_type;
#[cfg(feature = "vm-wasmer")]
mod wasmer_method;
#[cfg(feature = "vm-wasmtime")]
mod wasmtime_method;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, ImplItem::Method, ItemImpl};

#[proc_macro_attribute]
pub fn host_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Figure out namespace from attribute string
    let attribute = parse_macro_input!(attr as AttributeArgs);
    let namespace = match attribute::get_namespace(&attribute) {
        Ok(namespace) => namespace,
        Err(error) => return error,
    };

    // Check if type is compatible with the state
    let implementation = parse_macro_input!(item as ItemImpl);
    let self_ty = match state_type::check(&implementation.self_ty) {
        Ok(ident) => ident,
        Err(error) => return error,
    };

    // Create wrapper functions compatible with Wasmtime's runtime
    #[cfg(feature = "vm-wasmtime")]
    let mut wasmtime_method_wrappers = Vec::with_capacity(implementation.items.len());
    #[cfg(feature = "vm-wasmtime")]
    for item in implementation.items.iter() {
        match item {
            Method(method) => match wasmtime_method::wrap(namespace, method) {
                Ok(wrapper) => wasmtime_method_wrappers.push(wrapper),
                Err(error) => return error,
            },
            _ => (), // Ignore other items in the implementation
        }
    }
    #[allow(unused_variables)]
    let wasmtime_expanded = quote! {};
    #[cfg(feature = "vm-wasmtime")]
    let wasmtime_expanded = quote! {
        fn add_to_linker<E: 'static>(self, executor: E, linker: &mut wasmtime::Linker)
            where
                E: uptown_funk::Executor
            {
                let state = uptown_funk::StateWrapper::new(self, executor);
                #(#wasmtime_method_wrappers)*
            }
    };

    // Create wrapper functions compatible with Wasmer's runtime
    #[cfg(feature = "vm-wasmer")]
    let mut wasmer_method_wrappers = Vec::with_capacity(implementation.items.len());
    #[cfg(feature = "vm-wasmer")]
    for item in implementation.items.iter() {
        match item {
            Method(method) => match wasmer_method::wrap(namespace, method) {
                Ok(wrapper) => wasmer_method_wrappers.push(wrapper),
                Err(error) => return error,
            },
            _ => (), // Ignore other items in the implementation
        }
    }
    #[allow(unused_variables)]
    let wasmer_expanded = quote! {};
    #[cfg(feature = "vm-wasmer")]
    let wasmer_expanded = quote! {
        fn add_to_wasmer_linker<E: 'static>(
            self,
            executor: E,
            wasmer_linker: &mut uptown_funk::wasmer::WasmerLinker,
            store: &wasmer::Store,
        ) where
            E: uptown_funk::Executor,
        {
            let state = uptown_funk::StateWrapper::new(self, executor);
            #(#wasmer_method_wrappers)*
        }
    };

    let expanded = quote! {
        #implementation

        impl uptown_funk::HostFunctions for #self_ty {
            #wasmtime_expanded
            #wasmer_expanded
        }
    };

    TokenStream::from(expanded)
}
