use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{AttributeArgs, Lit, LitStr, Meta, NestedMeta};

pub fn get_namespace(attributes: &AttributeArgs) -> Result<&LitStr, TokenStream> {
    for kv in attributes.iter() {
        match kv {
            NestedMeta::Meta(meta) => match meta {
                Meta::NameValue(name_value) => {
                    let key = match name_value.path.segments.first() {
                        Some(path_segment) => &path_segment.ident,
                        None => return Err(namespace_error(kv)),
                    };
                    let namespace = match &name_value.lit {
                        Lit::Str(lit_str) => lit_str,
                        _ => return Err(namespace_error(kv)),
                    };
                    if key == "namespace" {
                        return Ok(namespace);
                    } else {
                        return Err(namespace_error(kv));
                    }
                }
                _ => return Err(namespace_error(kv)),
            },
            _ => return Err(namespace_error(kv)),
        }
    }

    Err(quote! {
        compile_error!("Attribute `namespace` is required for `#[uptown_funk::host_functions]`");
    }
    .into())
}

// Common error for namespace parsing
fn namespace_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!("Unsupported attribute for `#[uptown_funk::host_functions]` (only namespace=\"...\" is supported)");
    }).into()
}
