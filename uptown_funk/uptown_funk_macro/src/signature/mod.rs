mod inputs;
mod outputs;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{FnArg, ReturnType, Signature};

/// Takes a `signature` and returns a tuple of:
/// * Input signature of the wasm guest function.
/// * Return signature of the wasm guest function.
/// * Transformation steps from wasm guest arguments to host arguments.
/// * Signature of the host function.
/// * Transformation step from host return values to wasm guest returns.
pub fn transform(
    signature: &Signature,
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
    let mut input_arguments = signature.inputs.iter();
    // First element must match exactly `&self or &mut self`
    match input_arguments.next() {
        Some(FnArg::Receiver(receiver)) => {
            if receiver.reference.is_none() {
                return Err(self_error(receiver));
            }
        }
        None | Some(FnArg::Typed(_)) => return Err(self_error(signature)),
    };

    // Transform other input argumetns
    let mut guest_signature_input = Vec::new();
    let mut from_guest_input_transformations = Vec::new();
    let mut host_call_signature = Vec::new();

    for input_argument in input_arguments {
        match input_argument {
            FnArg::Typed(pat_type) => match inputs::transform(pat_type) {
                Ok((i, t, h)) => {
                    guest_signature_input.push(i);
                    from_guest_input_transformations.push(t);
                    host_call_signature.push(h)
                }
                Err(error) => return Err(error),
            },
            _ => return Err(self_error(signature)),
        }
    }

    // Transform return argument
    let return_argument = &signature.output;
    let (guest_signature_return, from_host_return_transformation) = match return_argument {
        ReturnType::Type(_, return_type) => match outputs::transform(&*return_type) {
            Ok((i, guest_signature_return, guest_to_host, h, host_to_guest)) => {
                guest_signature_input.push(i);
                from_guest_input_transformations.push(guest_to_host);
                host_call_signature.push(h);
                (guest_signature_return, host_to_guest)
            }
            Err(error) => return Err(error),
        },
        // No return type
        ReturnType::Default => (
            quote! { () },
            quote! { |_: ()| -> Result<(), uptown_funk::Trap> {Ok(())} },
        ),
    };

    let guest_signature_input = quote! { #(#guest_signature_input),* };
    let from_guest_input_transformations = quote! { #(#from_guest_input_transformations);* };
    let host_call_signature = quote! { #(#host_call_signature),* };

    Ok((
        guest_signature_input,
        guest_signature_return,
        from_guest_input_transformations,
        host_call_signature,
        from_host_return_transformation,
    ))
}

fn self_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!("The first argument for `#[uptown_funk::host_functions]` methods must be &self or &mut self.");
    })
    .into()
}

fn arg_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!("Unsupported argument for `#[uptown_funk::host_functions]` method.");
    })
    .into()
}

fn return_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!(
            "Unsupported return type for `#[uptown_funk::host_functions]` method. Supported types:
            * primitives -> i32, f32, ..
            * tuples of primitives -> (i32, i64, f32), ...
            * CustomType
            "
        );
    })
    .into()
}
