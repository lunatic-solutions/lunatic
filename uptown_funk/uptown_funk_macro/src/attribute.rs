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

                    if key != "namespace" {
                        continue;
                    }

                    let namespace = match &name_value.lit {
                        Lit::Str(lit_str) => lit_str,
                        _ => return Err(namespace_error(kv)),
                    };

                    return Ok(namespace);
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

#[derive(Debug, Copy, Clone)]
pub enum SyncType {
    None,
    Mutex,
}

pub fn get_sync(attributes: &AttributeArgs) -> Result<SyncType, TokenStream> {
    for kv in attributes.iter() {
        match kv {
            NestedMeta::Meta(meta) => match meta {
                Meta::NameValue(name_value) => {
                    let key = match name_value.path.segments.first() {
                        Some(path_segment) => &path_segment.ident,
                        None => return Err(sync_error(kv)),
                    };

                    if key != "sync" {
                        continue;
                    }

                    let namespace = match &name_value.lit {
                        Lit::Str(lit_str) => {
                            match lit_str.value().as_str() {
                                "none" => SyncType::None,
                                "mutex" => SyncType::Mutex,
                                _ => return Err(sync_error(kv))
                            }
                        },
                        _ => return Err(sync_error(kv)),
                    };

                    return Ok(namespace);
                }
                _ => return Err(sync_error(kv)),
            },
            _ => return Err(sync_error(kv)),
        }
    }

    Ok(SyncType::None)
}

// Common error for namespace parsing
fn namespace_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!("Unsupported attribute for `#[uptown_funk::host_functions]` (only namespace=\"...\" is supported)");
    }).into()
}

// Common error for sync parsing
fn sync_error<S: Spanned>(location: S) -> TokenStream {
    (quote_spanned! {
        location.span() =>
        compile_error!("Unsupported attribute for `#[uptown_funk::host_functions]` (only sync=\"none|mutex\" is supported)");
    }).into()
}