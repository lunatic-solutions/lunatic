mod attribute;
mod signature;
mod state_type;
#[cfg(feature = "vm-wasmtime")]
mod wasmtime_method;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, ImplItem::Method, ItemImpl};

use crate::attribute::SyncType;

#[proc_macro_attribute]
pub fn host_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Figure out namespace from attribute string
    let attribute = parse_macro_input!(attr as AttributeArgs);
    let namespace = match attribute::get_namespace(&attribute) {
        Ok(namespace) => namespace,
        Err(error) => return error,
    };

    let sync = match attribute::get_sync(&attribute) {
        Ok(sync) => sync,
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
            Method(method) => match wasmtime_method::wrap(namespace, sync, method) {
                Ok(wrapper) => wasmtime_method_wrappers.push(wrapper),
                Err(error) => return error,
            },
            _ => (), // Ignore other items in the implementation
        }
    }

    let prepare_state = match sync {
        SyncType::None => quote! {
            let api = ::std::rc::Rc::new(::std::cell::RefCell::new(api));
        },
        SyncType::Mutex => quote! {},
    };

    #[allow(unused_variables)]
    let wasmtime_expanded = quote! {};
    #[cfg(feature = "vm-wasmtime")]
    let wasmtime_expanded = quote! {
        fn add_to_linker<E: 'static>(api: Self::Wrap, executor: E, linker: &mut wasmtime::Linker)
            where
                E: uptown_funk::Executor + 'static
            {
                let executor = ::std::rc::Rc::new(executor);
                #prepare_state;
                #(#wasmtime_method_wrappers)*
            }
    };

    let assoc_types_and_split = match sync {
        SyncType::None => quote! {
            type Return = ();
            type Wrap = Self;

            fn split(self) -> (Self::Return, Self::Wrap) {
                ((), self)
            }
        },
        SyncType::Mutex => quote! {
            type Return = ::std::sync::Arc<::std::sync::Mutex<Self>>;
            type Wrap = ::std::sync::Arc<::std::sync::Mutex<Self>>;

            fn split(self) -> (Self::Return, Self::Wrap) {
                let s = ::std::sync::Arc::new(::std::sync::Mutex::new(self));
                (s.clone(), s)
            }
        },
    };

    let expanded = quote! {
        #implementation

        impl uptown_funk::HostFunctions for #self_ty {
            #assoc_types_and_split

            #wasmtime_expanded
        }
    };

    TokenStream::from(expanded)
}
