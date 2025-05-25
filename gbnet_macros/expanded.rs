#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Data, Fields, Index, GenericParam, Generics, Field,
    Type,
};
fn add_trait_bounds(
    mut generics: Generics,
    bound: proc_macro2::TokenStream,
) -> Generics {
    let parsed_bound: syn::TypeParamBound = syn::parse2(bound).unwrap();
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(parsed_bound.clone());
        }
    }
    generics
}
fn should_serialize_field(field: &Field) -> bool {
    !field.attrs.iter().any(|attr| attr.path().is_ident("no_serialize"))
}
fn get_field_bits(field: &Field) -> Option<usize> {
    field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("bits"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(
                    syn::MetaNameValue {
                        value: syn::Expr::Lit(
                            syn::ExprLit { lit: syn::Lit::Int(lit), .. },
                        ),
                        ..
                    },
                ) => lit.base10_parse::<usize>().ok(),
                _ => None,
            }
        })
}
fn get_max_len(field: &Field, input: &DeriveInput) -> Option<usize> {
    let field_max_len = field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("max_len"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(
                    syn::MetaNameValue {
                        value: syn::Expr::Lit(
                            syn::ExprLit { lit: syn::Lit::Int(lit), .. },
                        ),
                        ..
                    },
                ) => {
                    let result = lit.base10_parse::<usize>().ok();
                    {
                        ::std::io::_eprint(
                            format_args!(
                                "Field max_len for {0:?}: {1:?}\n",
                                field.ident,
                                result,
                            ),
                        );
                    };
                    result
                }
                _ => {
                    {
                        ::std::io::_eprint(
                            format_args!(
                                "Field max_len parse failed for {0:?}\n",
                                field.ident,
                            ),
                        );
                    };
                    None
                }
            }
        });
    if field_max_len.is_none() {
        let default_max_len = input
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("default_max_len"))
            .and_then(|attr| {
                match &attr.meta {
                    syn::Meta::NameValue(
                        syn::MetaNameValue {
                            value: syn::Expr::Lit(
                                syn::ExprLit { lit: syn::Lit::Int(lit), .. },
                            ),
                            ..
                        },
                    ) => {
                        let result = lit.base10_parse::<usize>().ok();
                        {
                            ::std::io::_eprint(
                                format_args!("Default max_len for input: {0:?}\n", result),
                            );
                        };
                        result
                    }
                    _ => {
                        {
                            ::std::io::_eprint(
                                format_args!("Default max_len parse failed\n"),
                            );
                        };
                        None
                    }
                }
            });
        return default_max_len;
    }
    field_max_len
}
fn is_byte_aligned(field: &Field) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident("byte_align"))
}
fn is_vec_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        type_path.path.segments.iter().any(|segment| segment.ident == "Vec")
    } else {
        false
    }
}
fn get_default_bits(input: &DeriveInput) -> Vec<(String, usize)> {
    input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("default_bits"))
        .flat_map(|attr| {
            attr.parse_args_with(
                    syn::punctuated::Punctuated::<
                        syn::Meta,
                        ::syn::token::Comma,
                    >::parse_terminated,
                )
                .unwrap_or_default()
                .into_iter()
                .filter_map(|meta| {
                    if let syn::Meta::NameValue(nv) = meta {
                        if let syn::Expr::Lit(expr_lit) = nv.value {
                            if let syn::Lit::Int(lit) = expr_lit.lit {
                                let type_name = nv.path.get_ident()?.to_string();
                                let bits = lit.base10_parse::<usize>().ok()?;
                                Some((type_name, bits))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
        })
        .collect()
}
fn get_field_bit_width(field: &Field, defaults: &[(String, usize)]) -> usize {
    if let Some(bits) = get_field_bits(field) {
        validate_field_bits(field, bits).expect("Invalid bits attribute");
        bits
    } else {
        let type_name = match &field.ty {
            Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
            _ => None,
        };
        if let Some(type_name) = &type_name {
            if let Some((_, bits)) = defaults.iter().find(|(t, _)| t == type_name) {
                validate_field_bits(field, *bits).expect("Invalid default bits");
                return *bits;
            }
        }
        match type_name.as_deref() {
            Some("u8") | Some("i8") => 8,
            Some("u16") | Some("i16") => 16,
            Some("u32") | Some("i32") => 32,
            Some("u64") | Some("i64") => 64,
            Some("f32") => 32,
            Some("f64") => 64,
            Some("bool") => 1,
            _ => 0,
        }
    }
}
fn validate_field_bits(field: &Field, bits: usize) -> syn::Result<()> {
    if bits > 64 {
        return Err(syn::Error::new_spanned(&field.ty, "Bits attribute exceeds 64"));
    }
    match &field.ty {
        Type::Path(type_path) => {
            let ident = type_path.path.get_ident().map(|i| i.to_string());
            match ident.as_deref() {
                Some("bool") if bits != 1 => {
                    Err(
                        syn::Error::new_spanned(&field.ty, "Bool requires exactly 1 bit"),
                    )
                }
                Some("u8") | Some("i8") if bits > 8 => {
                    Err(syn::Error::new_spanned(&field.ty, "Bits exceed u8/i8 capacity"))
                }
                Some("u16") | Some("i16") if bits > 16 => {
                    Err(
                        syn::Error::new_spanned(
                            &field.ty,
                            "Bits exceed u16/i16 capacity",
                        ),
                    )
                }
                Some("u32") | Some("i32") if bits > 32 => {
                    Err(
                        syn::Error::new_spanned(
                            &field.ty,
                            "Bits exceed u32/i32 capacity",
                        ),
                    )
                }
                Some("u64") | Some("i64") if bits > 64 => {
                    Err(
                        syn::Error::new_spanned(
                            &field.ty,
                            "Bits exceed u64/i64 capacity",
                        ),
                    )
                }
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}
fn get_enum_bits(input: &DeriveInput) -> Option<usize> {
    input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("bits"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(
                    syn::MetaNameValue {
                        value: syn::Expr::Lit(
                            syn::ExprLit { lit: syn::Lit::Int(lit), .. },
                        ),
                        ..
                    },
                ) => lit.base10_parse::<usize>().ok(),
                _ => None,
            }
        })
}
#[proc_macro_derive(
    NetworkSerialize,
    attributes(no_serialize, bits, max_len, byte_align, default_bits, default_max_len)
)]
pub fn derive_network_serialize(input: TokenStream) -> TokenStream {
    let input = match ::syn::parse::<DeriveInput>(input) {
        ::syn::__private::Ok(data) => data,
        ::syn::__private::Err(err) => {
            return ::syn::__private::TokenStream::from(err.to_compile_error());
        }
    };
    let name = &input.ident;
    let bit_serialize_impl = generate_bit_serialize_impl(&input, name);
    let bit_deserialize_impl = generate_bit_deserialize_impl(&input, name);
    let byte_aligned_serialize_impl = generate_byte_aligned_serialize_impl(&input, name);
    let byte_aligned_deserialize_impl = generate_byte_aligned_deserialize_impl(
        &input,
        name,
    );
    let expanded = {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::ToTokens::to_tokens(&bit_serialize_impl, &mut _s);
        ::quote::ToTokens::to_tokens(&bit_deserialize_impl, &mut _s);
        ::quote::ToTokens::to_tokens(&byte_aligned_serialize_impl, &mut _s);
        ::quote::ToTokens::to_tokens(&byte_aligned_deserialize_impl, &mut _s);
        _s
    };
    TokenStream::from(expanded)
}
fn generate_bit_serialize_impl(
    input: &DeriveInput,
    name: &syn::Ident,
) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(
        input.generics.clone(),
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "crate");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "serialize");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "BitSerialize");
            _s
        },
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let serialize_body = match &input.data {
        Data::Struct(data) => generate_struct_serialize(&data.fields, true, input),
        Data::Enum(data) => generate_enum_serialize(data, true, input),
        Data::Union(_) => {
            ::core::panicking::panic_fmt(format_args!("Unions are not supported"));
        }
    };
    {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::__private::push_ident(&mut _s, "impl");
        ::quote::ToTokens::to_tokens(&impl_generics, &mut _s);
        ::quote::__private::push_ident(&mut _s, "crate");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "serialize");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "BitSerialize");
        ::quote::__private::push_ident(&mut _s, "for");
        ::quote::ToTokens::to_tokens(&name, &mut _s);
        ::quote::ToTokens::to_tokens(&ty_generics, &mut _s);
        ::quote::ToTokens::to_tokens(&where_clause, &mut _s);
        ::quote::__private::push_group(
            &mut _s,
            ::quote::__private::Delimiter::Brace,
            {
                let mut _s = ::quote::__private::TokenStream::new();
                ::quote::__private::push_ident(&mut _s, "fn");
                ::quote::__private::push_ident(&mut _s, "bit_serialize");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "W");
                ::quote::__private::push_colon(&mut _s);
                ::quote::__private::push_ident(&mut _s, "crate");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "serialize");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "bit_io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "BitWrite");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "self");
                        ::quote::__private::push_comma(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "writer");
                        ::quote::__private::push_colon(&mut _s);
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "mut");
                        ::quote::__private::push_ident(&mut _s, "W");
                        _s
                    },
                );
                ::quote::__private::push_rarrow(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Result");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    ::quote::__private::TokenStream::new(),
                );
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Brace,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&serialize_body, &mut _s);
                        _s
                    },
                );
                _s
            },
        );
        _s
    }
}
fn generate_bit_deserialize_impl(
    input: &DeriveInput,
    name: &syn::Ident,
) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(
        input.generics.clone(),
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "crate");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "serialize");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "BitDeserialize");
            _s
        },
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let deserialize_body = match &input.data {
        Data::Struct(data) => generate_struct_deserialize(&data.fields, true, input),
        Data::Enum(data) => generate_enum_deserialize(data, true, input),
        Data::Union(_) => {
            ::core::panicking::panic_fmt(format_args!("Unions are not supported"));
        }
    };
    {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::__private::push_ident(&mut _s, "impl");
        ::quote::ToTokens::to_tokens(&impl_generics, &mut _s);
        ::quote::__private::push_ident(&mut _s, "crate");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "serialize");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "BitDeserialize");
        ::quote::__private::push_ident(&mut _s, "for");
        ::quote::ToTokens::to_tokens(&name, &mut _s);
        ::quote::ToTokens::to_tokens(&ty_generics, &mut _s);
        ::quote::ToTokens::to_tokens(&where_clause, &mut _s);
        ::quote::__private::push_group(
            &mut _s,
            ::quote::__private::Delimiter::Brace,
            {
                let mut _s = ::quote::__private::TokenStream::new();
                ::quote::__private::push_ident(&mut _s, "fn");
                ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "R");
                ::quote::__private::push_colon(&mut _s);
                ::quote::__private::push_ident(&mut _s, "crate");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "serialize");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "bit_io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "BitRead");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "reader");
                        ::quote::__private::push_colon(&mut _s);
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "mut");
                        ::quote::__private::push_ident(&mut _s, "R");
                        _s
                    },
                );
                ::quote::__private::push_rarrow(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Result");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Self");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Brace,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&deserialize_body, &mut _s);
                        _s
                    },
                );
                _s
            },
        );
        _s
    }
}
fn generate_byte_aligned_serialize_impl(
    input: &DeriveInput,
    name: &syn::Ident,
) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(
        input.generics.clone(),
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "crate");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "serialize");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "ByteAlignedSerialize");
            _s
        },
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let serialize_body = match &input.data {
        Data::Struct(data) => generate_struct_serialize(&data.fields, false, input),
        Data::Enum(data) => generate_enum_serialize(data, false, input),
        Data::Union(_) => {
            ::core::panicking::panic_fmt(format_args!("Unions are not supported"));
        }
    };
    {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::__private::push_ident(&mut _s, "impl");
        ::quote::ToTokens::to_tokens(&impl_generics, &mut _s);
        ::quote::__private::push_ident(&mut _s, "crate");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "serialize");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "ByteAlignedSerialize");
        ::quote::__private::push_ident(&mut _s, "for");
        ::quote::ToTokens::to_tokens(&name, &mut _s);
        ::quote::ToTokens::to_tokens(&ty_generics, &mut _s);
        ::quote::ToTokens::to_tokens(&where_clause, &mut _s);
        ::quote::__private::push_group(
            &mut _s,
            ::quote::__private::Delimiter::Brace,
            {
                let mut _s = ::quote::__private::TokenStream::new();
                ::quote::__private::push_ident(&mut _s, "fn");
                ::quote::__private::push_ident(&mut _s, "byte_aligned_serialize");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "W");
                ::quote::__private::push_colon(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Write");
                ::quote::__private::push_add(&mut _s);
                ::quote::__private::push_ident(&mut _s, "byteorder");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "WriteBytesExt");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "self");
                        ::quote::__private::push_comma(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "writer");
                        ::quote::__private::push_colon(&mut _s);
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "mut");
                        ::quote::__private::push_ident(&mut _s, "W");
                        _s
                    },
                );
                ::quote::__private::push_rarrow(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Result");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    ::quote::__private::TokenStream::new(),
                );
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Brace,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&serialize_body, &mut _s);
                        _s
                    },
                );
                _s
            },
        );
        _s
    }
}
fn generate_byte_aligned_deserialize_impl(
    input: &DeriveInput,
    name: &syn::Ident,
) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(
        input.generics.clone(),
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "crate");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "serialize");
            ::quote::__private::push_colon2(&mut _s);
            ::quote::__private::push_ident(&mut _s, "ByteAlignedDeserialize");
            _s
        },
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let deserialize_body = match &input.data {
        Data::Struct(data) => generate_struct_deserialize(&data.fields, false, input),
        Data::Enum(data) => generate_enum_deserialize(data, false, input),
        Data::Union(_) => {
            ::core::panicking::panic_fmt(format_args!("Unions are not supported"));
        }
    };
    {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::__private::push_ident(&mut _s, "impl");
        ::quote::ToTokens::to_tokens(&impl_generics, &mut _s);
        ::quote::__private::push_ident(&mut _s, "crate");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "serialize");
        ::quote::__private::push_colon2(&mut _s);
        ::quote::__private::push_ident(&mut _s, "ByteAlignedDeserialize");
        ::quote::__private::push_ident(&mut _s, "for");
        ::quote::ToTokens::to_tokens(&name, &mut _s);
        ::quote::ToTokens::to_tokens(&ty_generics, &mut _s);
        ::quote::ToTokens::to_tokens(&where_clause, &mut _s);
        ::quote::__private::push_group(
            &mut _s,
            ::quote::__private::Delimiter::Brace,
            {
                let mut _s = ::quote::__private::TokenStream::new();
                ::quote::__private::push_ident(&mut _s, "fn");
                ::quote::__private::push_ident(&mut _s, "byte_aligned_deserialize");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "R");
                ::quote::__private::push_colon(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Read");
                ::quote::__private::push_add(&mut _s);
                ::quote::__private::push_ident(&mut _s, "byteorder");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "ReadBytesExt");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "reader");
                        ::quote::__private::push_colon(&mut _s);
                        ::quote::__private::push_and(&mut _s);
                        ::quote::__private::push_ident(&mut _s, "mut");
                        ::quote::__private::push_ident(&mut _s, "R");
                        _s
                    },
                );
                ::quote::__private::push_rarrow(&mut _s);
                ::quote::__private::push_ident(&mut _s, "std");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "io");
                ::quote::__private::push_colon2(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Result");
                ::quote::__private::push_lt(&mut _s);
                ::quote::__private::push_ident(&mut _s, "Self");
                ::quote::__private::push_gt(&mut _s);
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Brace,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&deserialize_body, &mut _s);
                        _s
                    },
                );
                _s
            },
        );
        _s
    }
}
fn generate_struct_serialize(
    fields: &Fields,
    is_bit: bool,
    input: &DeriveInput,
) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    match fields {
        Fields::Named(fields) => {
            let serialize_fields = fields
                .named
                .iter()
                .filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let value_expr = {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "self");
                            ::quote::__private::push_dot(&mut _s);
                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                            _s
                        };
                        let serialize_code = if is_bit {
                            if bits > 0 {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                    ::quote::__private::push_ident(&mut _s, "as");
                                    ::quote::__private::push_ident(&mut _s, "u64");
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "1u64");
                                            ::quote::__private::push_shl(&mut _s);
                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_sub(&mut _s);
                                    ::quote::__private::parse(&mut _s, "1");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Value {} exceeds {} bits for field {:?}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "stringify");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "writer");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "write_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_comma(&mut _s);
                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil()
                                        as usize;
                                    (
                                        len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                            _s
                                        },
                                    )
                                } else {
                                    let default_len_bits = 16usize;
                                    (
                                        default_len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "65535usize");
                                            _s
                                        },
                                    )
                                };
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        ::quote::__private::TokenStream::new(),
                                    );
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "log");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "debug");
                                            ::quote::__private::push_bang(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(
                                                        &mut _s,
                                                        "\"Vector length {} exceeds max_len {} for field {:?}\"",
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "self");
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        ::quote::__private::TokenStream::new(),
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "stringify");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Vector length {} exceeds max_len {}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "self");
                                                                    ::quote::__private::push_dot(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                    ::quote::__private::push_dot(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        ::quote::__private::TokenStream::new(),
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "writer");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "write_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "self");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                ::quote::__private::TokenStream::new(),
                                            );
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_comma(&mut _s);
                                            ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "for");
                                    ::quote::__private::push_ident(&mut _s, "item");
                                    ::quote::__private::push_ident(&mut _s, "in");
                                    ::quote::__private::push_and(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "item");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    _s
                                }
                            } else {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            }
                        } else {
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "self");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "byte_aligned_serialize",
                                );
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        _s
                                    },
                                );
                                ::quote::__private::push_question(&mut _s);
                                ::quote::__private::push_semi(&mut _s);
                                _s
                            }
                        };
                        if is_byte_align && is_bit {
                            Some({
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "while");
                                ::quote::__private::push_ident(&mut _s, "writer");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "bit_pos");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    ::quote::__private::TokenStream::new(),
                                );
                                ::quote::__private::push_rem(&mut _s);
                                ::quote::__private::parse(&mut _s, "8");
                                ::quote::__private::push_ne(&mut _s);
                                ::quote::__private::parse(&mut _s, "0");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Brace,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "write_bit");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "false");
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    },
                                );
                                ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                _s
                            })
                        } else {
                            Some(serialize_code)
                        }
                    } else {
                        None
                    }
                });
            {
                let mut _s = ::quote::__private::TokenStream::new();
                {
                    use ::quote::__private::ext::*;
                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                    #[allow(unused_mut)]
                    let (mut serialize_fields, i) = serialize_fields.quote_into_iter();
                    let has_iter = has_iter | i;
                    let _: ::quote::__private::HasIterator = has_iter;
                    while true {
                        let serialize_fields = match serialize_fields.next() {
                            Some(_x) => ::quote::__private::RepInterp(_x),
                            None => break,
                        };
                        ::quote::ToTokens::to_tokens(&serialize_fields, &mut _s);
                    }
                }
                ::quote::__private::push_ident(&mut _s, "Ok");
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Parenthesis,
                            ::quote::__private::TokenStream::new(),
                        );
                        _s
                    },
                );
                _s
            }
        }
        Fields::Unnamed(fields) => {
            let serialize_fields = (0..fields.unnamed.len())
                .filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i]) {
                        let index = Index::from(i);
                        let is_byte_align = is_byte_aligned(&fields.unnamed[i]);
                        let bits = get_field_bit_width(&fields.unnamed[i], &defaults);
                        let max_len = get_max_len(&fields.unnamed[i], input);
                        let value_expr = {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "self");
                            ::quote::__private::push_dot(&mut _s);
                            ::quote::ToTokens::to_tokens(&index, &mut _s);
                            _s
                        };
                        let serialize_code = if is_bit {
                            if bits > 0 {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                    ::quote::__private::push_ident(&mut _s, "as");
                                    ::quote::__private::push_ident(&mut _s, "u64");
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "1u64");
                                            ::quote::__private::push_shl(&mut _s);
                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_sub(&mut _s);
                                    ::quote::__private::parse(&mut _s, "1");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Value {} exceeds {} bits for field {}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "writer");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "write_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&value_expr, &mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_comma(&mut _s);
                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            } else if is_vec_type(&fields.unnamed[i].ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil()
                                        as usize;
                                    (
                                        len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                            _s
                                        },
                                    )
                                } else {
                                    let default_len_bits = 16usize;
                                    (
                                        default_len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "65535usize");
                                            _s
                                        },
                                    )
                                };
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        ::quote::__private::TokenStream::new(),
                                    );
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "log");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "debug");
                                            ::quote::__private::push_bang(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(
                                                        &mut _s,
                                                        "\"Vector length {} exceeds max_len {} for field {}\"",
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "self");
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        ::quote::__private::TokenStream::new(),
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Vector length {} exceeds max_len {}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "self");
                                                                    ::quote::__private::push_dot(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                                                    ::quote::__private::push_dot(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        ::quote::__private::TokenStream::new(),
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "max_len");
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "writer");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "write_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "self");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::ToTokens::to_tokens(&index, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                ::quote::__private::TokenStream::new(),
                                            );
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_comma(&mut _s);
                                            ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "for");
                                    ::quote::__private::push_ident(&mut _s, "item");
                                    ::quote::__private::push_ident(&mut _s, "in");
                                    ::quote::__private::push_and(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "item");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    _s
                                }
                            } else {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "self");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::ToTokens::to_tokens(&index, &mut _s);
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            }
                        } else {
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "self");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::ToTokens::to_tokens(&index, &mut _s);
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "byte_aligned_serialize",
                                );
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        _s
                                    },
                                );
                                ::quote::__private::push_question(&mut _s);
                                ::quote::__private::push_semi(&mut _s);
                                _s
                            }
                        };
                        if is_byte_align && is_bit {
                            Some({
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "while");
                                ::quote::__private::push_ident(&mut _s, "writer");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "bit_pos");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    ::quote::__private::TokenStream::new(),
                                );
                                ::quote::__private::push_rem(&mut _s);
                                ::quote::__private::parse(&mut _s, "8");
                                ::quote::__private::push_ne(&mut _s);
                                ::quote::__private::parse(&mut _s, "0");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Brace,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "write_bit");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "false");
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    },
                                );
                                ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                _s
                            })
                        } else {
                            Some(serialize_code)
                        }
                    } else {
                        None
                    }
                });
            {
                let mut _s = ::quote::__private::TokenStream::new();
                {
                    use ::quote::__private::ext::*;
                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                    #[allow(unused_mut)]
                    let (mut serialize_fields, i) = serialize_fields.quote_into_iter();
                    let has_iter = has_iter | i;
                    let _: ::quote::__private::HasIterator = has_iter;
                    while true {
                        let serialize_fields = match serialize_fields.next() {
                            Some(_x) => ::quote::__private::RepInterp(_x),
                            None => break,
                        };
                        ::quote::ToTokens::to_tokens(&serialize_fields, &mut _s);
                    }
                }
                ::quote::__private::push_ident(&mut _s, "Ok");
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Parenthesis,
                            ::quote::__private::TokenStream::new(),
                        );
                        _s
                    },
                );
                _s
            }
        }
        Fields::Unit => {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "Ok");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Parenthesis,
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        ::quote::__private::TokenStream::new(),
                    );
                    _s
                },
            );
            _s
        }
    }
}
fn generate_struct_deserialize(
    fields: &Fields,
    is_bit: bool,
    input: &DeriveInput,
) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    match fields {
        Fields::Named(fields) => {
            let field_names = fields
                .named
                .iter()
                .filter_map(|f| {
                    if should_serialize_field(f) {
                        f.ident.as_ref().map(|ident| ident.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let field_defaults = fields
                .named
                .iter()
                .filter_map(|f| {
                    if !should_serialize_field(f) {
                        f.ident
                            .as_ref()
                            .map(|ident| {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::ToTokens::to_tokens(&ident, &mut _s);
                                ::quote::__private::push_colon(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "Default");
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "default");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    ::quote::__private::TokenStream::new(),
                                );
                                _s
                            })
                    } else {
                        None
                    }
                });
            let deserialize_fields = fields
                .named
                .iter()
                .filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let type_name = match &f.ty {
                            Type::Path(type_path) => {
                                type_path.path.get_ident().map(|i| i.to_string())
                            }
                            _ => None,
                        };
                        let deserialize_code = if is_bit {
                            if bits > 0 {
                                if type_name.as_deref() == Some("bool") {
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "let");
                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                        ::quote::__private::push_eq(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bits");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    }
                                } else {
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "let");
                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                        ::quote::__private::push_eq(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bits");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "as");
                                        ::quote::__private::push_underscore(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    }
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil()
                                        as usize;
                                    (
                                        len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                            _s
                                        },
                                    )
                                } else {
                                    let default_len_bits = 16usize;
                                    (
                                        default_len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "65535usize");
                                            _s
                                        },
                                    )
                                };
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "reader");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "read_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "as");
                                    ::quote::__private::push_ident(&mut _s, "usize");
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "log");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "debug");
                                            ::quote::__private::push_bang(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(
                                                        &mut _s,
                                                        "\"Vector length {} exceeds max_len {} for field {:?}\"",
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "stringify");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Vector length {} exceeds max_len {}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "mut");
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "Vec");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "with_capacity");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "for");
                                    ::quote::__private::push_underscore(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "in");
                                    ::quote::__private::parse(&mut _s, "0");
                                    ::quote::__private::push_dot2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "push");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "crate");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "serialize");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "reader");
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_question(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    _s
                                }
                            } else {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "crate");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "serialize");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "reader");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            }
                        } else {
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "let");
                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                ::quote::__private::push_eq(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "crate");
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "serialize");
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "ByteAlignedDeserialize",
                                );
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "byte_aligned_deserialize",
                                );
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        _s
                                    },
                                );
                                ::quote::__private::push_question(&mut _s);
                                ::quote::__private::push_semi(&mut _s);
                                _s
                            }
                        };
                        if is_byte_align && is_bit {
                            Some({
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "while");
                                ::quote::__private::push_ident(&mut _s, "reader");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "bit_pos");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    ::quote::__private::TokenStream::new(),
                                );
                                ::quote::__private::push_rem(&mut _s);
                                ::quote::__private::parse(&mut _s, "8");
                                ::quote::__private::push_ne(&mut _s);
                                ::quote::__private::parse(&mut _s, "0");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Brace,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bit");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    },
                                );
                                ::quote::ToTokens::to_tokens(&deserialize_code, &mut _s);
                                _s
                            })
                        } else {
                            Some(deserialize_code)
                        }
                    } else {
                        None
                    }
                });
            {
                let mut _s = ::quote::__private::TokenStream::new();
                {
                    use ::quote::__private::ext::*;
                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                    #[allow(unused_mut)]
                    let (mut deserialize_fields, i) = deserialize_fields
                        .quote_into_iter();
                    let has_iter = has_iter | i;
                    let _: ::quote::__private::HasIterator = has_iter;
                    while true {
                        let deserialize_fields = match deserialize_fields.next() {
                            Some(_x) => ::quote::__private::RepInterp(_x),
                            None => break,
                        };
                        ::quote::ToTokens::to_tokens(&deserialize_fields, &mut _s);
                    }
                }
                ::quote::__private::push_ident(&mut _s, "Ok");
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "Self");
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_names, i) = field_names.quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_names = match field_names.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                        ::quote::__private::push_comma(&mut _s);
                                    }
                                }
                                {
                                    use ::quote::__private::ext::*;
                                    let mut _i = 0usize;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_defaults, i) = field_defaults
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_defaults = match field_defaults.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        if _i > 0 {
                                            ::quote::__private::push_comma(&mut _s);
                                        }
                                        _i += 1;
                                        ::quote::ToTokens::to_tokens(&field_defaults, &mut _s);
                                    }
                                }
                                _s
                            },
                        );
                        _s
                    },
                );
                _s
            }
        }
        Fields::Unnamed(fields) => {
            let field_names = (0..fields.unnamed.len())
                .filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i]) {
                        Some(
                            syn::Ident::new(
                                &::alloc::__export::must_use({
                                    let res = ::alloc::fmt::format(
                                        format_args!("field_{0}", i),
                                    );
                                    res
                                }),
                                proc_macro2::Span::call_site(),
                            ),
                        )
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let field_defaults = (0..fields.unnamed.len())
                .filter_map(|i| {
                    if !should_serialize_field(&fields.unnamed[i]) {
                        Some({
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "Default");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "default");
                            ::quote::__private::push_group(
                                &mut _s,
                                ::quote::__private::Delimiter::Parenthesis,
                                ::quote::__private::TokenStream::new(),
                            );
                            _s
                        })
                    } else {
                        None
                    }
                });
            let deserialize_fields = fields
                .unnamed
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    let name = syn::Ident::new(
                        &::alloc::__export::must_use({
                            let res = ::alloc::fmt::format(format_args!("field_{0}", i));
                            res
                        }),
                        proc_macro2::Span::call_site(),
                    );
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let type_name = match &f.ty {
                            Type::Path(type_path) => {
                                type_path.path.get_ident().map(|i| i.to_string())
                            }
                            _ => None,
                        };
                        let deserialize_code = if is_bit {
                            if bits > 0 {
                                if type_name.as_deref() == Some("bool") {
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "let");
                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                        ::quote::__private::push_eq(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bits");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    }
                                } else {
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "let");
                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                        ::quote::__private::push_eq(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bits");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "as");
                                        ::quote::__private::push_underscore(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    }
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil()
                                        as usize;
                                    (
                                        len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                            _s
                                        },
                                    )
                                } else {
                                    let default_len_bits = 16usize;
                                    (
                                        default_len_bits,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::parse(&mut _s, "65535usize");
                                            _s
                                        },
                                    )
                                };
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "reader");
                                    ::quote::__private::push_dot(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "read_bits");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "as");
                                    ::quote::__private::push_ident(&mut _s, "usize");
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "if");
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_gt(&mut _s);
                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "log");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "debug");
                                            ::quote::__private::push_bang(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(
                                                        &mut _s,
                                                        "\"Vector length {} exceeds max_len {} for field {}\"",
                                                    );
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&i, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "return");
                                            ::quote::__private::push_ident(&mut _s, "Err");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "Error");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "new");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "format");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::parse(
                                                                        &mut _s,
                                                                        "\"Vector length {} exceeds max_len {}\"",
                                                                    );
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::__private::push_ident(&mut _s, "mut");
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "Vec");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "with_capacity");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_semi(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "for");
                                    ::quote::__private::push_underscore(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "in");
                                    ::quote::__private::parse(&mut _s, "0");
                                    ::quote::__private::push_dot2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "len");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Brace,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "push");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "crate");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "serialize");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "reader");
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_question(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        },
                                    );
                                    _s
                                }
                            } else {
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "let");
                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                    ::quote::__private::push_eq(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "crate");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "serialize");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "reader");
                                            _s
                                        },
                                    );
                                    ::quote::__private::push_question(&mut _s);
                                    ::quote::__private::push_semi(&mut _s);
                                    _s
                                }
                            }
                        } else {
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "let");
                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                ::quote::__private::push_eq(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "crate");
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "serialize");
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "ByteAlignedDeserialize",
                                );
                                ::quote::__private::push_colon2(&mut _s);
                                ::quote::__private::push_ident(
                                    &mut _s,
                                    "byte_aligned_deserialize",
                                );
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        _s
                                    },
                                );
                                ::quote::__private::push_question(&mut _s);
                                ::quote::__private::push_semi(&mut _s);
                                _s
                            }
                        };
                        if is_byte_align && is_bit {
                            Some({
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::__private::push_ident(&mut _s, "while");
                                ::quote::__private::push_ident(&mut _s, "reader");
                                ::quote::__private::push_dot(&mut _s);
                                ::quote::__private::push_ident(&mut _s, "bit_pos");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    ::quote::__private::TokenStream::new(),
                                );
                                ::quote::__private::push_rem(&mut _s);
                                ::quote::__private::parse(&mut _s, "8");
                                ::quote::__private::push_ne(&mut _s);
                                ::quote::__private::parse(&mut _s, "0");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Brace,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "read_bit");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_question(&mut _s);
                                        ::quote::__private::push_semi(&mut _s);
                                        _s
                                    },
                                );
                                ::quote::ToTokens::to_tokens(&deserialize_code, &mut _s);
                                _s
                            })
                        } else {
                            Some(deserialize_code)
                        }
                    } else {
                        None
                    }
                });
            {
                let mut _s = ::quote::__private::TokenStream::new();
                {
                    use ::quote::__private::ext::*;
                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                    #[allow(unused_mut)]
                    let (mut deserialize_fields, i) = deserialize_fields
                        .quote_into_iter();
                    let has_iter = has_iter | i;
                    let _: ::quote::__private::HasIterator = has_iter;
                    while true {
                        let deserialize_fields = match deserialize_fields.next() {
                            Some(_x) => ::quote::__private::RepInterp(_x),
                            None => break,
                        };
                        ::quote::ToTokens::to_tokens(&deserialize_fields, &mut _s);
                    }
                }
                ::quote::__private::push_ident(&mut _s, "Ok");
                ::quote::__private::push_group(
                    &mut _s,
                    ::quote::__private::Delimiter::Parenthesis,
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "Self");
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Parenthesis,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_names, i) = field_names.quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_names = match field_names.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                        ::quote::__private::push_comma(&mut _s);
                                    }
                                }
                                {
                                    use ::quote::__private::ext::*;
                                    let mut _i = 0usize;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_defaults, i) = field_defaults
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_defaults = match field_defaults.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        if _i > 0 {
                                            ::quote::__private::push_comma(&mut _s);
                                        }
                                        _i += 1;
                                        ::quote::ToTokens::to_tokens(&field_defaults, &mut _s);
                                    }
                                }
                                _s
                            },
                        );
                        _s
                    },
                );
                _s
            }
        }
        Fields::Unit => {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "Ok");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Parenthesis,
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::__private::push_ident(&mut _s, "Self");
                    _s
                },
            );
            _s
        }
    }
}
fn generate_enum_serialize(
    data: &syn::DataEnum,
    is_bit: bool,
    input: &DeriveInput,
) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    let variant_count = data.variants.len();
    let min_bits = if variant_count == 0 {
        0
    } else {
        (variant_count as f64).log2().ceil() as usize
    };
    let bits = get_enum_bits(input).unwrap_or(min_bits);
    if bits < min_bits {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Enum bits attribute ({0}) too small to represent {1} variants (needs at least {2})",
                    bits,
                    variant_count,
                    min_bits,
                ),
            );
        };
    }
    if bits > 64 {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Enum bits attribute ({0}) exceeds 64, too large for variant index",
                    bits,
                ),
            );
        };
    }
    if !is_bit && variant_count > 256 {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Too many enum variants ({0}) for byte-aligned serialization (max 256)",
                    variant_count,
                ),
            );
        };
    }
    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let variant_index = i as u64;
            let serialize_code = if is_bit {
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::__private::push_ident(&mut _s, "writer");
                    ::quote::__private::push_dot(&mut _s);
                    ::quote::__private::push_ident(&mut _s, "write_bits");
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::ToTokens::to_tokens(&variant_index, &mut _s);
                            ::quote::__private::push_comma(&mut _s);
                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                            _s
                        },
                    );
                    ::quote::__private::push_question(&mut _s);
                    ::quote::__private::push_semi(&mut _s);
                    _s
                }
            } else {
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::__private::push_ident(&mut _s, "writer");
                    ::quote::__private::push_dot(&mut _s);
                    ::quote::__private::push_ident(&mut _s, "write_u8");
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::ToTokens::to_tokens(&variant_index, &mut _s);
                            ::quote::__private::push_ident(&mut _s, "as");
                            ::quote::__private::push_ident(&mut _s, "u8");
                            _s
                        },
                    );
                    ::quote::__private::push_question(&mut _s);
                    ::quote::__private::push_semi(&mut _s);
                    _s
                }
            };
            match &variant.fields {
                Fields::Named(fields) => {
                    let field_names = fields
                        .named
                        .iter()
                        .map(|f| f.ident.as_ref().unwrap())
                        .collect::<Vec<_>>();
                    let serialize_fields = fields
                        .named
                        .iter()
                        .filter_map(|f| {
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f) {
                                let is_byte_align = is_byte_aligned(f);
                                let bits = get_field_bit_width(f, &defaults);
                                let max_len = get_max_len(f, input);
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => {
                                        type_path.path.get_ident().map(|i| i.to_string())
                                    }
                                    _ => None,
                                };
                                let serialize_code = if is_bit {
                                    if bits > 0 {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::__private::push_star(&mut _s);
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "1u64");
                                                    ::quote::__private::push_shl(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_sub(&mut _s);
                                            ::quote::__private::parse(&mut _s, "1");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Value {} exceeds {} bits for field {:?}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_star(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "stringify");
                                                                            ::quote::__private::push_bang(&mut _s);
                                                                            ::quote::__private::push_group(
                                                                                &mut _s,
                                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                                {
                                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                                    _s
                                                                                },
                                                                            );
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "write_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_star(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "as");
                                                    ::quote::__private::push_ident(&mut _s, "u64");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    } else if is_vec_type(&f.ty) {
                                        let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                            let len_bits = ((max_len + 1) as f64).log2().ceil()
                                                as usize;
                                            (
                                                len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                                    _s
                                                },
                                            )
                                        } else {
                                            let default_len_bits = 16usize;
                                            (
                                                default_len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "65535usize");
                                                    _s
                                                },
                                            )
                                        };
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                ::quote::__private::TokenStream::new(),
                                            );
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "log");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "debug");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::parse(
                                                                &mut _s,
                                                                "\"Vector length {} exceeds max_len {} for field {:?}\"",
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                            ::quote::__private::push_dot(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                ::quote::__private::TokenStream::new(),
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "stringify");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Vector length {} exceeds max_len {}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                            ::quote::__private::push_dot(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                                            ::quote::__private::push_group(
                                                                                &mut _s,
                                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                                ::quote::__private::TokenStream::new(),
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "write_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        ::quote::__private::TokenStream::new(),
                                                    );
                                                    ::quote::__private::push_ident(&mut _s, "as");
                                                    ::quote::__private::push_ident(&mut _s, "u64");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "for");
                                            ::quote::__private::push_ident(&mut _s, "item");
                                            ::quote::__private::push_ident(&mut _s, "in");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "item");
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "writer");
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_question(&mut _s);
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            _s
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                } else {
                                    if bits > 0 {
                                        match type_name.as_deref() {
                                            Some("u8") | Some("i8") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u16") | Some("i16") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u16");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u32") | Some("i32") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u32");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u64") | Some("i64") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u64");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("bool") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "if");
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        ::quote::__private::push_group(
                                                            &mut _s,
                                                            ::quote::__private::Delimiter::Brace,
                                                            {
                                                                let mut _s = ::quote::__private::TokenStream::new();
                                                                ::quote::__private::parse(&mut _s, "1");
                                                                _s
                                                            },
                                                        );
                                                        ::quote::__private::push_ident(&mut _s, "else");
                                                        ::quote::__private::push_group(
                                                            &mut _s,
                                                            ::quote::__private::Delimiter::Brace,
                                                            {
                                                                let mut _s = ::quote::__private::TokenStream::new();
                                                                ::quote::__private::parse(&mut _s, "0");
                                                                _s
                                                            },
                                                        );
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            _ => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "byte_aligned_serialize",
                                                );
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "writer");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "byte_aligned_serialize",
                                            );
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                };
                                if is_byte_align && is_bit {
                                    Some({
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "while");
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "bit_pos");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_rem(&mut _s);
                                        ::quote::__private::parse(&mut _s, "8");
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Brace,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_bit");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "false");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                        _s
                                    })
                                } else {
                                    Some(serialize_code)
                                }
                            } else {
                                None
                            }
                        });
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "Self");
                        ::quote::__private::push_colon2(&mut _s);
                        ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let mut _i = 0usize;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_names, i) = field_names.quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_names = match field_names.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        if _i > 0 {
                                            ::quote::__private::push_comma(&mut _s);
                                        }
                                        _i += 1;
                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                    }
                                }
                                _s
                            },
                        );
                        ::quote::__private::push_fat_arrow(&mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut serialize_fields, i) = serialize_fields
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let serialize_fields = match serialize_fields.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&serialize_fields, &mut _s);
                                    }
                                }
                                ::quote::__private::push_ident(&mut _s, "Ok");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        _s
                                    },
                                );
                                _s
                            },
                        );
                        _s
                    }
                }
                Fields::Unnamed(fields) => {
                    let field_names = (0..fields.unnamed.len())
                        .map(|i| syn::Ident::new(
                            &::alloc::__export::must_use({
                                let res = ::alloc::fmt::format(
                                    format_args!("field_{0}", i),
                                );
                                res
                            }),
                            proc_macro2::Span::call_site(),
                        ))
                        .collect::<Vec<_>>();
                    let serialize_fields = fields
                        .unnamed
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            let name = &field_names[i];
                            if should_serialize_field(f) {
                                let is_byte_align = is_byte_aligned(f);
                                let bits = get_field_bit_width(f, &defaults);
                                let max_len = get_max_len(f, input);
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => {
                                        type_path.path.get_ident().map(|i| i.to_string())
                                    }
                                    _ => None,
                                };
                                let serialize_code = if is_bit {
                                    if bits > 0 {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::__private::push_star(&mut _s);
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "u64");
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "1u64");
                                                    ::quote::__private::push_shl(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_sub(&mut _s);
                                            ::quote::__private::parse(&mut _s, "1");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Value {} exceeds {} bits for field {}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_star(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&i, &mut _s);
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "write_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_star(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "as");
                                                    ::quote::__private::push_ident(&mut _s, "u64");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    } else if is_vec_type(&f.ty) {
                                        let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                            let len_bits = ((max_len + 1) as f64).log2().ceil()
                                                as usize;
                                            (
                                                len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                                    _s
                                                },
                                            )
                                        } else {
                                            let default_len_bits = 16usize;
                                            (
                                                default_len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "65535usize");
                                                    _s
                                                },
                                            )
                                        };
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                ::quote::__private::TokenStream::new(),
                                            );
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "log");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "debug");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::parse(
                                                                &mut _s,
                                                                "\"Vector length {} exceeds max_len {} for field {}\"",
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                            ::quote::__private::push_dot(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                ::quote::__private::TokenStream::new(),
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&i, &mut _s);
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Vector length {} exceeds max_len {}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                            ::quote::__private::push_dot(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                                            ::quote::__private::push_group(
                                                                                &mut _s,
                                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                                ::quote::__private::TokenStream::new(),
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "max_len");
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "writer");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "write_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        ::quote::__private::TokenStream::new(),
                                                    );
                                                    ::quote::__private::push_ident(&mut _s, "as");
                                                    ::quote::__private::push_ident(&mut _s, "u64");
                                                    ::quote::__private::push_comma(&mut _s);
                                                    ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "for");
                                            ::quote::__private::push_ident(&mut _s, "item");
                                            ::quote::__private::push_ident(&mut _s, "in");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "item");
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "writer");
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_question(&mut _s);
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            _s
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_serialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                } else {
                                    if bits > 0 {
                                        match type_name.as_deref() {
                                            Some("u8") | Some("i8") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u16") | Some("i16") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u16");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u32") | Some("i32") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u32");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u64") | Some("i64") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u64");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("bool") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "if");
                                                        ::quote::__private::push_star(&mut _s);
                                                        ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                        ::quote::__private::push_group(
                                                            &mut _s,
                                                            ::quote::__private::Delimiter::Brace,
                                                            {
                                                                let mut _s = ::quote::__private::TokenStream::new();
                                                                ::quote::__private::parse(&mut _s, "1");
                                                                _s
                                                            },
                                                        );
                                                        ::quote::__private::push_ident(&mut _s, "else");
                                                        ::quote::__private::push_group(
                                                            &mut _s,
                                                            ::quote::__private::Delimiter::Brace,
                                                            {
                                                                let mut _s = ::quote::__private::TokenStream::new();
                                                                ::quote::__private::parse(&mut _s, "0");
                                                                _s
                                                            },
                                                        );
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            _ => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "byte_aligned_serialize",
                                                );
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "writer");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "byte_aligned_serialize",
                                            );
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "writer");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                };
                                if is_byte_align && is_bit {
                                    Some({
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "while");
                                        ::quote::__private::push_ident(&mut _s, "writer");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "bit_pos");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_rem(&mut _s);
                                        ::quote::__private::parse(&mut _s, "8");
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Brace,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "writer");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "write_bit");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "false");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                        _s
                                    })
                                } else {
                                    Some(serialize_code)
                                }
                            } else {
                                None
                            }
                        });
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::__private::push_ident(&mut _s, "Self");
                        ::quote::__private::push_colon2(&mut _s);
                        ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Parenthesis,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let mut _i = 0usize;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut field_names, i) = field_names.quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let field_names = match field_names.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        if _i > 0 {
                                            ::quote::__private::push_comma(&mut _s);
                                        }
                                        _i += 1;
                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                    }
                                }
                                _s
                            },
                        );
                        ::quote::__private::push_fat_arrow(&mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut serialize_fields, i) = serialize_fields
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let serialize_fields = match serialize_fields.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&serialize_fields, &mut _s);
                                    }
                                }
                                ::quote::__private::push_ident(&mut _s, "Ok");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        _s
                                    },
                                );
                                _s
                            },
                        );
                        _s
                    }
                }
                Fields::Unit => {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::__private::push_ident(&mut _s, "Self");
                    ::quote::__private::push_colon2(&mut _s);
                    ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                    ::quote::__private::push_fat_arrow(&mut _s);
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Brace,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::ToTokens::to_tokens(&serialize_code, &mut _s);
                            ::quote::__private::push_ident(&mut _s, "Ok");
                            ::quote::__private::push_group(
                                &mut _s,
                                ::quote::__private::Delimiter::Parenthesis,
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        ::quote::__private::TokenStream::new(),
                                    );
                                    _s
                                },
                            );
                            _s
                        },
                    );
                    _s
                }
            }
        });
    {
        let mut _s = ::quote::__private::TokenStream::new();
        ::quote::__private::push_ident(&mut _s, "match");
        ::quote::__private::push_ident(&mut _s, "self");
        ::quote::__private::push_group(
            &mut _s,
            ::quote::__private::Delimiter::Brace,
            {
                let mut _s = ::quote::__private::TokenStream::new();
                {
                    use ::quote::__private::ext::*;
                    let mut _i = 0usize;
                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                    #[allow(unused_mut)]
                    let (mut variants, i) = variants.quote_into_iter();
                    let has_iter = has_iter | i;
                    let _: ::quote::__private::HasIterator = has_iter;
                    while true {
                        let variants = match variants.next() {
                            Some(_x) => ::quote::__private::RepInterp(_x),
                            None => break,
                        };
                        if _i > 0 {
                            ::quote::__private::push_comma(&mut _s);
                        }
                        _i += 1;
                        ::quote::ToTokens::to_tokens(&variants, &mut _s);
                    }
                }
                _s
            },
        );
        _s
    }
}
fn generate_enum_deserialize(
    data: &syn::DataEnum,
    is_bit: bool,
    input: &DeriveInput,
) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    let variant_count = data.variants.len();
    let min_bits = if variant_count == 0 {
        0
    } else {
        (variant_count as f64).log2().ceil() as usize
    };
    let bits = get_enum_bits(input).unwrap_or(min_bits);
    if bits < min_bits {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Enum bits attribute ({0}) too small to represent {1} variants (needs at least {2})",
                    bits,
                    variant_count,
                    min_bits,
                ),
            );
        };
    }
    if bits > 64 {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Enum bits attribute ({0}) exceeds 64, too large for variant index",
                    bits,
                ),
            );
        };
    }
    if !is_bit && variant_count > 256 {
        {
            ::core::panicking::panic_fmt(
                format_args!(
                    "Too many enum variants ({0}) for byte-aligned serialization (max 256)",
                    variant_count,
                ),
            );
        };
    }
    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let variant_index = i as u64;
            match &variant.fields {
                Fields::Named(fields) => {
                    let field_names = fields
                        .named
                        .iter()
                        .filter_map(|f| {
                            if should_serialize_field(f) {
                                f.ident.as_ref().map(|ident| ident.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let field_defaults = fields
                        .named
                        .iter()
                        .filter_map(|f| {
                            if !should_serialize_field(f) {
                                f.ident
                                    .as_ref()
                                    .map(|ident| {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::ToTokens::to_tokens(&ident, &mut _s);
                                        ::quote::__private::push_colon(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "Default");
                                        ::quote::__private::push_colon2(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "default");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        _s
                                    })
                            } else {
                                None
                            }
                        });
                    let deserialize_fields = fields
                        .named
                        .iter()
                        .filter_map(|f| {
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f) {
                                let is_byte_align = is_byte_aligned(f);
                                let bits = get_field_bit_width(f, &defaults);
                                let max_len = get_max_len(f, input);
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => {
                                        type_path.path.get_ident().map(|i| i.to_string())
                                    }
                                    _ => None,
                                };
                                let deserialize_code = if is_bit {
                                    if bits > 0 {
                                        if type_name.as_deref() == Some("bool") {
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bits");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ne(&mut _s);
                                                ::quote::__private::parse(&mut _s, "0");
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        } else {
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bits");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "as");
                                                ::quote::__private::push_underscore(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else if is_vec_type(&f.ty) {
                                        let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                            let len_bits = ((max_len + 1) as f64).log2().ceil()
                                                as usize;
                                            (
                                                len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                                    _s
                                                },
                                            )
                                        } else {
                                            let default_len_bits = 16usize;
                                            (
                                                default_len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "65535usize");
                                                    _s
                                                },
                                            )
                                        };
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "reader");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "read_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "usize");
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "log");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "debug");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::parse(
                                                                &mut _s,
                                                                "\"Vector length {} exceeds max_len {} for field {:?}\"",
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "stringify");
                                                            ::quote::__private::push_bang(&mut _s);
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Vector length {} exceeds max_len {}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "mut");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "Vec");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "with_capacity");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "for");
                                            ::quote::__private::push_underscore(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "in");
                                            ::quote::__private::parse(&mut _s, "0");
                                            ::quote::__private::push_dot2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "push");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "crate");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                                    _s
                                                                },
                                                            );
                                                            ::quote::__private::push_question(&mut _s);
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            _s
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "crate");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                } else {
                                    if bits > 0 {
                                        match type_name.as_deref() {
                                            Some("u8") | Some("i8") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u16") | Some("i16") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u16");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u32") | Some("i32") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u32");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u64") | Some("i64") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u64");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("bool") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ne(&mut _s);
                                                ::quote::__private::parse(&mut _s, "0");
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            _ => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "crate");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "serialize");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "ByteAlignedDeserialize",
                                                );
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "byte_aligned_deserialize",
                                                );
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "reader");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "crate");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "ByteAlignedDeserialize",
                                            );
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "byte_aligned_deserialize",
                                            );
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                };
                                if is_byte_align && is_bit {
                                    Some({
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "while");
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "bit_pos");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_rem(&mut _s);
                                        ::quote::__private::parse(&mut _s, "8");
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Brace,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bit");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::ToTokens::to_tokens(&deserialize_code, &mut _s);
                                        _s
                                    })
                                } else {
                                    Some(deserialize_code)
                                }
                            } else {
                                None
                            }
                        });
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&variant_index, &mut _s);
                        ::quote::__private::push_fat_arrow(&mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut deserialize_fields, i) = deserialize_fields
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let deserialize_fields = match deserialize_fields.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&deserialize_fields, &mut _s);
                                    }
                                }
                                ::quote::__private::push_ident(&mut _s, "Ok");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "Self");
                                        ::quote::__private::push_colon2(&mut _s);
                                        ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Brace,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                {
                                                    use ::quote::__private::ext::*;
                                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                                    #[allow(unused_mut)]
                                                    let (mut field_names, i) = field_names.quote_into_iter();
                                                    let has_iter = has_iter | i;
                                                    let _: ::quote::__private::HasIterator = has_iter;
                                                    while true {
                                                        let field_names = match field_names.next() {
                                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                                            None => break,
                                                        };
                                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                                        ::quote::__private::push_comma(&mut _s);
                                                    }
                                                }
                                                {
                                                    use ::quote::__private::ext::*;
                                                    let mut _i = 0usize;
                                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                                    #[allow(unused_mut)]
                                                    let (mut field_defaults, i) = field_defaults
                                                        .quote_into_iter();
                                                    let has_iter = has_iter | i;
                                                    let _: ::quote::__private::HasIterator = has_iter;
                                                    while true {
                                                        let field_defaults = match field_defaults.next() {
                                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                                            None => break,
                                                        };
                                                        if _i > 0 {
                                                            ::quote::__private::push_comma(&mut _s);
                                                        }
                                                        _i += 1;
                                                        ::quote::ToTokens::to_tokens(&field_defaults, &mut _s);
                                                    }
                                                }
                                                _s
                                            },
                                        );
                                        _s
                                    },
                                );
                                _s
                            },
                        );
                        _s
                    }
                }
                Fields::Unnamed(fields) => {
                    let field_names = (0..fields.unnamed.len())
                        .filter_map(|i| {
                            if should_serialize_field(&fields.unnamed[i]) {
                                Some(
                                    syn::Ident::new(
                                        &::alloc::__export::must_use({
                                            let res = ::alloc::fmt::format(
                                                format_args!("field_{0}", i),
                                            );
                                            res
                                        }),
                                        proc_macro2::Span::call_site(),
                                    ),
                                )
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let field_defaults = (0..fields.unnamed.len())
                        .filter_map(|i| {
                            if !should_serialize_field(&fields.unnamed[i]) {
                                Some({
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "Default");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "default");
                                    ::quote::__private::push_group(
                                        &mut _s,
                                        ::quote::__private::Delimiter::Parenthesis,
                                        ::quote::__private::TokenStream::new(),
                                    );
                                    _s
                                })
                            } else {
                                None
                            }
                        });
                    let deserialize_fields = fields
                        .unnamed
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            let name = syn::Ident::new(
                                &::alloc::__export::must_use({
                                    let res = ::alloc::fmt::format(
                                        format_args!("field_{0}", i),
                                    );
                                    res
                                }),
                                proc_macro2::Span::call_site(),
                            );
                            if should_serialize_field(f) {
                                let is_byte_align = is_byte_aligned(f);
                                let bits = get_field_bit_width(f, &defaults);
                                let max_len = get_max_len(f, input);
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => {
                                        type_path.path.get_ident().map(|i| i.to_string())
                                    }
                                    _ => None,
                                };
                                let deserialize_code = if is_bit {
                                    if bits > 0 {
                                        if type_name.as_deref() == Some("bool") {
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bits");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ne(&mut _s);
                                                ::quote::__private::parse(&mut _s, "0");
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        } else {
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bits");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::ToTokens::to_tokens(&bits, &mut _s);
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "as");
                                                ::quote::__private::push_underscore(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else if is_vec_type(&f.ty) {
                                        let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                            let len_bits = ((max_len + 1) as f64).log2().ceil()
                                                as usize;
                                            (
                                                len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&max_len, &mut _s);
                                                    _s
                                                },
                                            )
                                        } else {
                                            let default_len_bits = 16usize;
                                            (
                                                default_len_bits,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::parse(&mut _s, "65535usize");
                                                    _s
                                                },
                                            )
                                        };
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "reader");
                                            ::quote::__private::push_dot(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "read_bits");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&len_bits, &mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "as");
                                            ::quote::__private::push_ident(&mut _s, "usize");
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "if");
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_gt(&mut _s);
                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "log");
                                                    ::quote::__private::push_colon2(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "debug");
                                                    ::quote::__private::push_bang(&mut _s);
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::parse(
                                                                &mut _s,
                                                                "\"Vector length {} exceeds max_len {} for field {}\"",
                                                            );
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                            ::quote::__private::push_comma(&mut _s);
                                                            ::quote::ToTokens::to_tokens(&i, &mut _s);
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "return");
                                                    ::quote::__private::push_ident(&mut _s, "Err");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "std");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "io");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "Error");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "new");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "std");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "io");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                                                    ::quote::__private::push_colon2(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                                                    ::quote::__private::push_comma(&mut _s);
                                                                    ::quote::__private::push_ident(&mut _s, "format");
                                                                    ::quote::__private::push_bang(&mut _s);
                                                                    ::quote::__private::push_group(
                                                                        &mut _s,
                                                                        ::quote::__private::Delimiter::Parenthesis,
                                                                        {
                                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                                            ::quote::__private::parse(
                                                                                &mut _s,
                                                                                "\"Vector length {} exceeds max_len {}\"",
                                                                            );
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::__private::push_ident(&mut _s, "len");
                                                                            ::quote::__private::push_comma(&mut _s);
                                                                            ::quote::ToTokens::to_tokens(&max_len_expr, &mut _s);
                                                                            _s
                                                                        },
                                                                    );
                                                                    _s
                                                                },
                                                            );
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::__private::push_ident(&mut _s, "mut");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "Vec");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "with_capacity");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "len");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_semi(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "for");
                                            ::quote::__private::push_underscore(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "in");
                                            ::quote::__private::parse(&mut _s, "0");
                                            ::quote::__private::push_dot2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "len");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Brace,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                    ::quote::__private::push_dot(&mut _s);
                                                    ::quote::__private::push_ident(&mut _s, "push");
                                                    ::quote::__private::push_group(
                                                        &mut _s,
                                                        ::quote::__private::Delimiter::Parenthesis,
                                                        {
                                                            let mut _s = ::quote::__private::TokenStream::new();
                                                            ::quote::__private::push_ident(&mut _s, "crate");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                                            ::quote::__private::push_colon2(&mut _s);
                                                            ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                                            ::quote::__private::push_group(
                                                                &mut _s,
                                                                ::quote::__private::Delimiter::Parenthesis,
                                                                {
                                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                                    _s
                                                                },
                                                            );
                                                            ::quote::__private::push_question(&mut _s);
                                                            _s
                                                        },
                                                    );
                                                    ::quote::__private::push_semi(&mut _s);
                                                    _s
                                                },
                                            );
                                            _s
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "crate");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "BitDeserialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "bit_deserialize");
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                } else {
                                    if bits > 0 {
                                        match type_name.as_deref() {
                                            Some("u8") | Some("i8") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u16") | Some("i16") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u16");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u32") | Some("i32") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u32");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("u64") | Some("i64") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u64");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_lt(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "LittleEndian");
                                                ::quote::__private::push_gt(&mut _s);
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            Some("bool") => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_u8");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_ne(&mut _s);
                                                ::quote::__private::parse(&mut _s, "0");
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                            _ => {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "let");
                                                ::quote::ToTokens::to_tokens(&name, &mut _s);
                                                ::quote::__private::push_eq(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "crate");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "serialize");
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "ByteAlignedDeserialize",
                                                );
                                                ::quote::__private::push_colon2(&mut _s);
                                                ::quote::__private::push_ident(
                                                    &mut _s,
                                                    "byte_aligned_deserialize",
                                                );
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    {
                                                        let mut _s = ::quote::__private::TokenStream::new();
                                                        ::quote::__private::push_ident(&mut _s, "reader");
                                                        _s
                                                    },
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            }
                                        }
                                    } else {
                                        {
                                            let mut _s = ::quote::__private::TokenStream::new();
                                            ::quote::__private::push_ident(&mut _s, "let");
                                            ::quote::ToTokens::to_tokens(&name, &mut _s);
                                            ::quote::__private::push_eq(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "crate");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(&mut _s, "serialize");
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "ByteAlignedDeserialize",
                                            );
                                            ::quote::__private::push_colon2(&mut _s);
                                            ::quote::__private::push_ident(
                                                &mut _s,
                                                "byte_aligned_deserialize",
                                            );
                                            ::quote::__private::push_group(
                                                &mut _s,
                                                ::quote::__private::Delimiter::Parenthesis,
                                                {
                                                    let mut _s = ::quote::__private::TokenStream::new();
                                                    ::quote::__private::push_ident(&mut _s, "reader");
                                                    _s
                                                },
                                            );
                                            ::quote::__private::push_question(&mut _s);
                                            ::quote::__private::push_semi(&mut _s);
                                            _s
                                        }
                                    }
                                };
                                if is_byte_align && is_bit {
                                    Some({
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "while");
                                        ::quote::__private::push_ident(&mut _s, "reader");
                                        ::quote::__private::push_dot(&mut _s);
                                        ::quote::__private::push_ident(&mut _s, "bit_pos");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            ::quote::__private::TokenStream::new(),
                                        );
                                        ::quote::__private::push_rem(&mut _s);
                                        ::quote::__private::parse(&mut _s, "8");
                                        ::quote::__private::push_ne(&mut _s);
                                        ::quote::__private::parse(&mut _s, "0");
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Brace,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                ::quote::__private::push_ident(&mut _s, "reader");
                                                ::quote::__private::push_dot(&mut _s);
                                                ::quote::__private::push_ident(&mut _s, "read_bit");
                                                ::quote::__private::push_group(
                                                    &mut _s,
                                                    ::quote::__private::Delimiter::Parenthesis,
                                                    ::quote::__private::TokenStream::new(),
                                                );
                                                ::quote::__private::push_question(&mut _s);
                                                ::quote::__private::push_semi(&mut _s);
                                                _s
                                            },
                                        );
                                        ::quote::ToTokens::to_tokens(&deserialize_code, &mut _s);
                                        _s
                                    })
                                } else {
                                    Some(deserialize_code)
                                }
                            } else {
                                None
                            }
                        });
                    {
                        let mut _s = ::quote::__private::TokenStream::new();
                        ::quote::ToTokens::to_tokens(&variant_index, &mut _s);
                        ::quote::__private::push_fat_arrow(&mut _s);
                        ::quote::__private::push_group(
                            &mut _s,
                            ::quote::__private::Delimiter::Brace,
                            {
                                let mut _s = ::quote::__private::TokenStream::new();
                                {
                                    use ::quote::__private::ext::*;
                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                    #[allow(unused_mut)]
                                    let (mut deserialize_fields, i) = deserialize_fields
                                        .quote_into_iter();
                                    let has_iter = has_iter | i;
                                    let _: ::quote::__private::HasIterator = has_iter;
                                    while true {
                                        let deserialize_fields = match deserialize_fields.next() {
                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                            None => break,
                                        };
                                        ::quote::ToTokens::to_tokens(&deserialize_fields, &mut _s);
                                    }
                                }
                                ::quote::__private::push_ident(&mut _s, "Ok");
                                ::quote::__private::push_group(
                                    &mut _s,
                                    ::quote::__private::Delimiter::Parenthesis,
                                    {
                                        let mut _s = ::quote::__private::TokenStream::new();
                                        ::quote::__private::push_ident(&mut _s, "Self");
                                        ::quote::__private::push_colon2(&mut _s);
                                        ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                                        ::quote::__private::push_group(
                                            &mut _s,
                                            ::quote::__private::Delimiter::Parenthesis,
                                            {
                                                let mut _s = ::quote::__private::TokenStream::new();
                                                {
                                                    use ::quote::__private::ext::*;
                                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                                    #[allow(unused_mut)]
                                                    let (mut field_names, i) = field_names.quote_into_iter();
                                                    let has_iter = has_iter | i;
                                                    let _: ::quote::__private::HasIterator = has_iter;
                                                    while true {
                                                        let field_names = match field_names.next() {
                                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                                            None => break,
                                                        };
                                                        ::quote::ToTokens::to_tokens(&field_names, &mut _s);
                                                        ::quote::__private::push_comma(&mut _s);
                                                    }
                                                }
                                                {
                                                    use ::quote::__private::ext::*;
                                                    let mut _i = 0usize;
                                                    let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                                                    #[allow(unused_mut)]
                                                    let (mut field_defaults, i) = field_defaults
                                                        .quote_into_iter();
                                                    let has_iter = has_iter | i;
                                                    let _: ::quote::__private::HasIterator = has_iter;
                                                    while true {
                                                        let field_defaults = match field_defaults.next() {
                                                            Some(_x) => ::quote::__private::RepInterp(_x),
                                                            None => break,
                                                        };
                                                        if _i > 0 {
                                                            ::quote::__private::push_comma(&mut _s);
                                                        }
                                                        _i += 1;
                                                        ::quote::ToTokens::to_tokens(&field_defaults, &mut _s);
                                                    }
                                                }
                                                _s
                                            },
                                        );
                                        _s
                                    },
                                );
                                _s
                            },
                        );
                        _s
                    }
                }
                Fields::Unit => {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::ToTokens::to_tokens(&variant_index, &mut _s);
                    ::quote::__private::push_fat_arrow(&mut _s);
                    ::quote::__private::push_ident(&mut _s, "Ok");
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "Self");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::ToTokens::to_tokens(&variant_name, &mut _s);
                            _s
                        },
                    );
                    _s
                }
            }
        });
    if is_bit {
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "let");
            ::quote::__private::push_ident(&mut _s, "variant_index");
            ::quote::__private::push_eq(&mut _s);
            ::quote::__private::push_ident(&mut _s, "reader");
            ::quote::__private::push_dot(&mut _s);
            ::quote::__private::push_ident(&mut _s, "read_bits");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Parenthesis,
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    ::quote::ToTokens::to_tokens(&bits, &mut _s);
                    _s
                },
            );
            ::quote::__private::push_question(&mut _s);
            ::quote::__private::push_semi(&mut _s);
            ::quote::__private::push_ident(&mut _s, "match");
            ::quote::__private::push_ident(&mut _s, "variant_index");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Brace,
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    {
                        use ::quote::__private::ext::*;
                        let mut _i = 0usize;
                        let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                        #[allow(unused_mut)]
                        let (mut variants, i) = variants.quote_into_iter();
                        let has_iter = has_iter | i;
                        let _: ::quote::__private::HasIterator = has_iter;
                        while true {
                            let variants = match variants.next() {
                                Some(_x) => ::quote::__private::RepInterp(_x),
                                None => break,
                            };
                            if _i > 0 {
                                ::quote::__private::push_comma(&mut _s);
                            }
                            _i += 1;
                            ::quote::ToTokens::to_tokens(&variants, &mut _s);
                        }
                    }
                    ::quote::__private::push_underscore(&mut _s);
                    ::quote::__private::push_fat_arrow(&mut _s);
                    ::quote::__private::push_ident(&mut _s, "Err");
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "std");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "io");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "Error");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "new");
                            ::quote::__private::push_group(
                                &mut _s,
                                ::quote::__private::Delimiter::Parenthesis,
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "std");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "io");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                    ::quote::__private::push_comma(&mut _s);
                                    ::quote::__private::parse(
                                        &mut _s,
                                        "\"Unknown variant index\"",
                                    );
                                    _s
                                },
                            );
                            _s
                        },
                    );
                    ::quote::__private::push_comma(&mut _s);
                    _s
                },
            );
            _s
        }
    } else {
        {
            let mut _s = ::quote::__private::TokenStream::new();
            ::quote::__private::push_ident(&mut _s, "let");
            ::quote::__private::push_ident(&mut _s, "variant_index");
            ::quote::__private::push_eq(&mut _s);
            ::quote::__private::push_ident(&mut _s, "reader");
            ::quote::__private::push_dot(&mut _s);
            ::quote::__private::push_ident(&mut _s, "read_u8");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Parenthesis,
                ::quote::__private::TokenStream::new(),
            );
            ::quote::__private::push_question(&mut _s);
            ::quote::__private::push_ident(&mut _s, "as");
            ::quote::__private::push_ident(&mut _s, "u64");
            ::quote::__private::push_semi(&mut _s);
            ::quote::__private::push_ident(&mut _s, "match");
            ::quote::__private::push_ident(&mut _s, "variant_index");
            ::quote::__private::push_group(
                &mut _s,
                ::quote::__private::Delimiter::Brace,
                {
                    let mut _s = ::quote::__private::TokenStream::new();
                    {
                        use ::quote::__private::ext::*;
                        let mut _i = 0usize;
                        let has_iter = ::quote::__private::ThereIsNoIteratorInRepetition;
                        #[allow(unused_mut)]
                        let (mut variants, i) = variants.quote_into_iter();
                        let has_iter = has_iter | i;
                        let _: ::quote::__private::HasIterator = has_iter;
                        while true {
                            let variants = match variants.next() {
                                Some(_x) => ::quote::__private::RepInterp(_x),
                                None => break,
                            };
                            if _i > 0 {
                                ::quote::__private::push_comma(&mut _s);
                            }
                            _i += 1;
                            ::quote::ToTokens::to_tokens(&variants, &mut _s);
                        }
                    }
                    ::quote::__private::push_underscore(&mut _s);
                    ::quote::__private::push_fat_arrow(&mut _s);
                    ::quote::__private::push_ident(&mut _s, "Err");
                    ::quote::__private::push_group(
                        &mut _s,
                        ::quote::__private::Delimiter::Parenthesis,
                        {
                            let mut _s = ::quote::__private::TokenStream::new();
                            ::quote::__private::push_ident(&mut _s, "std");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "io");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "Error");
                            ::quote::__private::push_colon2(&mut _s);
                            ::quote::__private::push_ident(&mut _s, "new");
                            ::quote::__private::push_group(
                                &mut _s,
                                ::quote::__private::Delimiter::Parenthesis,
                                {
                                    let mut _s = ::quote::__private::TokenStream::new();
                                    ::quote::__private::push_ident(&mut _s, "std");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "io");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "ErrorKind");
                                    ::quote::__private::push_colon2(&mut _s);
                                    ::quote::__private::push_ident(&mut _s, "InvalidData");
                                    ::quote::__private::push_comma(&mut _s);
                                    ::quote::__private::parse(
                                        &mut _s,
                                        "\"Unknown variant index\"",
                                    );
                                    _s
                                },
                            );
                            _s
                        },
                    );
                    ::quote::__private::push_comma(&mut _s);
                    _s
                },
            );
            _s
        }
    }
}
const _: () = {
    extern crate proc_macro;
    #[rustc_proc_macro_decls]
    #[used]
    #[allow(deprecated)]
    static _DECLS: &[proc_macro::bridge::client::ProcMacro] = &[
        proc_macro::bridge::client::ProcMacro::custom_derive(
            "NetworkSerialize",
            &[
                "no_serialize",
                "bits",
                "max_len",
                "byte_align",
                "default_bits",
                "default_max_len",
            ],
            derive_network_serialize,
        ),
    ];
};
