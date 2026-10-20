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

#[proc_macro_derive(FromByml, attributes(byml))]
pub fn derive_from_byml(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let result = (|| -> syn::Result<TokenStream2> {
        let name = &input.ident;
        let Data::Struct(data) = &input.data else {
            return Err(syn::Error::new_spanned(
                &input,
                "FromByml requires a struct",
            ));
        };

        let mut reads = Vec::new();
        for field in &data.fields {
            let attrs = parse_byml_attrs(&field.attrs)?;
            if attrs.skip {
                continue;
            }

            let ident = field
                .ident
                .as_ref()
                .ok_or_else(|| syn::Error::new_spanned(field, "named field required"))?;

            let key = attrs
                .key
                .unwrap_or_else(|| to_pascal_case(&ident.to_string()));

            reads.push(quote! {
                if let Some(value) = map.get(#key) {
                    result.#ident = crate::typed_param::decode(value)
                        .map_err(|e| ::anyhow::anyhow!("{}: {}", #key, e))?;
                }
            });
        }

        Ok(quote! {
            impl crate::typed_param::BymlDecode for #name {
                fn decode(value: &::tomolib::formats::byml::Value) -> ::anyhow::Result<Self> {
                    let ::tomolib::formats::byml::Value::Dict(map) = value else {
                        ::anyhow::bail!("expected a parameter dictionary");
                    };
                    let mut result = Self::default();
                    #(#reads)*
                    Ok(result)
                }
            }
        })
    })();

    result.unwrap_or_else(|e| e.to_compile_error()).into()
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
                let hash_val = murmur32(&raw_key);
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

    let full_insertions = fields.iter().map(|field| {
        let ident = field.ident.as_ref().unwrap();
        let attrs = parse_byml_attrs(&field.attrs)?;
        if attrs.skip { return Ok(quote! {}); }
        let key = attrs.key.unwrap_or_else(|| to_pascal_case(&ident.to_string()));

        let value = if let Some(via) = attrs.via {
            let via = format_ident!("{}", via);
            quote! { ::tomolib::formats::byml::Value::U32(self.#ident.#via()) }
        } else { type_to_value_expr(&field.ty, &quote! { self.#ident })? };

        let is_option = matches!(&field.ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Option"));

        Ok(if is_option {
            quote! { if self.#ident.is_some() { map.insert(#key.to_string(), #value); } }
        } else { quote! { map.insert(#key.to_string(), #value); } })

    }).collect::<syn::Result<Vec<_>>>()?;

    let field_names = fields
        .iter()
        .filter_map(|field| {
            let attrs = parse_byml_attrs(&field.attrs).ok()?;

            if attrs.skip {
                return None;
            }

            let ident = field.ident.as_ref()?.to_string();
            let key = attrs.key.unwrap_or_else(|| to_pascal_case(&ident));
            Some(quote! { (#ident, #key) })
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        impl #name {
            pub fn to_byml_full_map(&self) -> ::std::collections::BTreeMap<String, ::tomolib::formats::byml::Value> {
                let mut map = ::std::collections::BTreeMap::new();
                #(#full_insertions)*
                map
            }

            pub const BYML_FIELDS: &'static [(&'static str, &'static str)] = &[#(#field_names),*];

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
            let value_type_str = if type_str.starts_with("BTreeMap<") {
                &type_str["BTreeMap<String,".len()..type_str.len() - 1]
            } else {
                &type_str["std::collections::BTreeMap<String,".len()..type_str.len() - 1]
            };

            let map_value = match value_type_str.trim() {
                "bool" => quote! { ::tomolib::formats::byml::Value::Bool(*v) },
                "i32" => quote! { ::tomolib::formats::byml::Value::I32(*v) },
                "u32" => quote! { ::tomolib::formats::byml::Value::U32(*v) },
                "f32" => quote! { ::tomolib::formats::byml::Value::F32(*v) },
                "String" => quote! { ::tomolib::formats::byml::Value::String(v.clone()) },
                _ => quote! { v.to_byml() },
            };

            return Ok(quote! {
                ::tomolib::formats::byml::Value::Dict(
                    #value_expr.iter().map(|(k, v)| (k.clone(), #map_value)).collect()
                )
            });
        } else {
            return Ok(quote! {
                ::tomolib::formats::byml::Value::Hash32(
                    #value_expr.iter().map(|(k, v)| (k.hash_u32(), v.to_byml())).collect()
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

fn murmur32(s: &str) -> u32 {
    let mut hash = 0u32;
    let (chunks, remainder) = s.as_bytes().as_chunks::<4>();

    for chunk in chunks {
        let k = u32::from_le_bytes(*chunk)
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593);

        hash = (hash ^ k)
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe6546b64);
    }

    let mut tail = 0u32;

    for (i, b) in remainder.iter().enumerate() {
        tail |= u32::from(*b) << (8 * i);
    }

    if !remainder.is_empty() {
        hash ^= tail
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593);
    }

    hash ^= s.len() as u32;
    hash = (hash ^ (hash >> 16)).wrapping_mul(0x85ebca6b);
    hash = (hash ^ (hash >> 13)).wrapping_mul(0xc2b2ae35);
    hash ^ (hash >> 16)
}
