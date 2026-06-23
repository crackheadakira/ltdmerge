use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Fields, Lit, Meta, Type, parse_macro_input, punctuated::Punctuated,
    token::Comma,
};

/// Derives `ToByml` for a struct, generating a `to_byml_map()` method that
/// returns a `BTreeMap<String, tomolib::formats::byml::Value>`.
#[proc_macro_derive(ToByml, attributes(byml))]
pub fn derive_to_byml(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_to_byml(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn impl_to_byml(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "ToByml only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "ToByml can only be derived for structs",
            ));
        }
    };

    let insertions = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let attrs = parse_byml_attrs(&field.attrs)?;

            if attrs.skip {
                return Ok(quote! {});
            }

            if attrs.flatten {
                return Ok(quote! {
                    map.extend(self.#field_name.to_byml_map());
                });
            }

            let raw_key = attrs
                .key
                .clone()
                .unwrap_or_else(|| to_pascal_case(&field_name.to_string()));

            let key = if attrs.hash {
                let hash_val = murmurhash3_32(raw_key.as_bytes(), 0);
                format!("{:08x}", hash_val)
            } else {
                raw_key
            };

            let value_expr = if let Some(ref via) = attrs.via {
                let via_ident = format_ident!("{}", via);
                quote! { self.#field_name.#via_ident() }
            } else {
                quote! { self.#field_name }
            };

            let value_variant = if attrs.via.is_some() {
                let via = attrs.via.as_deref().unwrap_or("");
                if via.contains("u32") {
                    quote! { ::tomolib::formats::byml::Value::U32(#value_expr) }
                } else if via.contains("i32") {
                    quote! { ::tomolib::formats::byml::Value::I32(#value_expr) }
                } else if via.contains("f32") {
                    quote! { ::tomolib::formats::byml::Value::F32(#value_expr) }
                } else {
                    quote! { #value_expr.to_byml() }
                }
            } else {
                type_to_value_expr(&field.ty, &value_expr)?
            };

            if attrs.skip_none {
                Ok(quote! {
                    if self.#field_name.is_some() {
                        map.insert(#key.to_string(), #value_variant);
                    }
                })
            } else if let Some(ref default_val) = attrs.default {
                Ok(quote! {
                            if #value_expr != #default_val {
                    map.insert(#key.to_string(), #value_variant);
                }
                        })
            } else {
                Ok(quote! {
                    map.insert(#key.to_string(), #value_variant);
                })
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {
        impl #name {
            pub fn to_byml_map(&self) -> ::std::collections::BTreeMap<String, ::tomolib::formats::byml::Value> {
                let mut map = ::std::collections::BTreeMap::new();
                #(#insertions)*
                map
            }

            pub fn to_byml(&self) -> ::tomolib::formats::byml::Value {
                ::tomolib::formats::byml::Value::Dict(self.to_byml_map())
            }
        }
    })
}

fn type_to_value_expr(ty: &Type, value_expr: &TokenStream2) -> syn::Result<TokenStream2> {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    match type_str.trim() {
        "bool" => return Ok(quote! { ::tomolib::formats::byml::Value::Bool(#value_expr) }),
        "i32" => return Ok(quote! { ::tomolib::formats::byml::Value::I32(#value_expr) }),
        "u32" => return Ok(quote! { ::tomolib::formats::byml::Value::U32(#value_expr) }),
        "f32" => return Ok(quote! { ::tomolib::formats::byml::Value::F32(#value_expr) }),
        "String" => {
            return Ok(quote! { ::tomolib::formats::byml::Value::String(#value_expr.clone()) });
        }
        _ => {}
    }

    if type_str.starts_with("BTreeMap<") || type_str.starts_with("std::collections::BTreeMap<") {
        if type_str.contains("String,") {
            return Ok(quote! {
                ::tomolib::formats::byml::Value::Dict(
                    #value_expr.iter().map(|(k, v)| (k.clone(), v.to_byml())).collect()
                )
            });
        } else {
            return Ok(quote! {
                ::tomolib::formats::byml::Value::Hash32(
                    #value_expr.iter().map(|(&k, v)| (k as u32, v.to_byml())).collect()
                )
            });
        }
    }

    if type_str.starts_with("Vec<") {
        return Ok(quote! {
            ::tomolib::formats::byml::Value::Array(
                #value_expr.iter().map(|v| v.to_byml()).collect()
            )
        });
    }

    if type_str.starts_with("Option<") && type_str.ends_with('>') {
        let inner_type_str = type_str[7..type_str.len() - 1].trim();

        let (inner_tokens, is_primitive) = match inner_type_str {
            "bool" => (
                quote! { ::tomolib::formats::byml::Value::Bool(*inner) },
                true,
            ),
            "i32" => (
                quote! { ::tomolib::formats::byml::Value::I32(*inner) },
                true,
            ),
            "u32" => (
                quote! { ::tomolib::formats::byml::Value::U32(*inner) },
                true,
            ),
            "f32" => (
                quote! { ::tomolib::formats::byml::Value::F32(*inner) },
                true,
            ),
            "String" => (
                quote! { ::tomolib::formats::byml::Value::String(inner.clone()) },
                true,
            ),
            _ => (quote! { inner.to_byml() }, false),
        };

        if is_primitive {
            let fallback_tokens = match inner_type_str {
                "bool" => quote! { ::tomolib::formats::byml::Value::Bool(false) },
                "i32" => quote! { ::tomolib::formats::byml::Value::I32(0) },
                "u32" => quote! { ::tomolib::formats::byml::Value::U32(0) },
                "f32" => quote! { ::tomolib::formats::byml::Value::F32(0.0) },
                _ => {
                    quote! { ::tomolib::formats::byml::Value::String(::std::string::String::new()) }
                }
            };

            return Ok(quote! {
                if let Some(ref inner) = #value_expr {
                    #inner_tokens
                } else {
                    #fallback_tokens
                }
            });
        } else {
            return Ok(quote! {
                if let Some(ref inner) = #value_expr {
                    #inner_tokens
                } else {
                    ::tomolib::formats::byml::Value::Dict(::std::collections::BTreeMap::new())
                }
            });
        }
    }

    Ok(quote! { #value_expr.to_byml() })
}

#[derive(Default)]
struct BymlAttrs {
    key: Option<String>,
    via: Option<String>,
    default: Option<syn::Expr>,
    skip: bool,
    flatten: bool,
    hash: bool,
    skip_none: bool,
}

fn parse_byml_attrs(attrs: &[syn::Attribute]) -> syn::Result<BymlAttrs> {
    let mut result = BymlAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("byml") {
            continue;
        }

        let nested = attr.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;

        for meta in nested {
            match &meta {
                Meta::Path(p) if p.is_ident("skip") => {
                    result.skip = true;
                }

                Meta::Path(p) if p.is_ident("flatten") => {
                    result.flatten = true;
                }

                Meta::Path(p) if p.is_ident("skip_none") => {
                    result.skip_none = true;
                }

                Meta::Path(p) if p.is_ident("hash") => {
                    result.hash = true;
                }

                Meta::NameValue(nv) if nv.path.is_ident("key") => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }) = &nv.value
                    {
                        result.key = Some(s.value());
                    }
                }

                Meta::NameValue(nv) if nv.path.is_ident("via") => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }) = &nv.value
                    {
                        result.via = Some(s.value());
                    }
                }

                Meta::NameValue(nv) if nv.path.is_ident("default") => {
                    result.default = Some(nv.value.clone());
                }

                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "unknown byml attribute — expected `key`, `via`, `skip`, or `flatten`",
                    ));
                }
            }
        }
    }

    Ok(result)
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

fn murmurhash3_32(key: &[u8], seed: u32) -> u32 {
    let mut hash = seed;
    let chunks = key.chunks_exact(4);
    let remainder = chunks.remainder();

    const C1: u32 = 0xcc9e2d51;
    const C2: u32 = 0x1b873593;

    for chunk in chunks {
        let mut k = u32::from_le_bytes(chunk.try_into().unwrap());

        k = k.wrapping_mul(C1);
        k = k.rotate_left(15);
        k = k.wrapping_mul(C2);

        hash ^= k;
        hash = hash.rotate_left(13);
        hash = hash.wrapping_mul(5).wrapping_add(0xe6546b64);
    }

    let mut k1: u32 = 0;
    if !remainder.is_empty() {
        if remainder.len() >= 3 {
            k1 ^= (remainder[2] as u32) << 16;
        }
        if remainder.len() >= 2 {
            k1 ^= (remainder[1] as u32) << 8;
        }
        if remainder.len() >= 1 {
            k1 ^= remainder[0] as u32;
        }

        k1 = k1.wrapping_mul(C1);
        k1 = k1.rotate_left(15);
        k1 = k1.wrapping_mul(C2);
        hash ^= k1;
    }

    hash ^= key.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^= hash >> 16;

    hash
}
