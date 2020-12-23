use proc_macro::TokenStream;
use quote::quote_spanned;
use syn::spanned::Spanned;
use syn::{Ident, Type};

/// Check if the type implementing uptown_funk::HostFunctions is allowed to be captured as Wasm
/// instance state. (Only simple paths are currently supported, e.g. `Networking`, `Porcesses`).
pub fn check(state_type: &Type) -> Result<&Ident, TokenStream> {
    match state_type {
        Type::Path(type_path) => match type_path.path.get_ident() {
            Some(ident) => Ok(ident),
            None => Err(quote_spanned! {
                state_type.span() =>
                compile_error!("Unsupported type path for `#[uptown_funk::host_functions]` state");
            }
            .into()),
        },
        _ => Err(quote_spanned! {
            state_type.span() =>
            compile_error!("Unsupported type for `#[uptown_funk::host_functions]` state");
        }
        .into()),
    }
}
