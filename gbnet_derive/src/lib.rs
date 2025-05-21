use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Index, GenericParam, Generics, Field};

#[proc_macro_derive(Serialize, attributes(serialize_all, serialize, no_serialize, bits))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { Serialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));
    let bits_attr = input.attrs.iter().find(|attr| attr.path().is_ident("bits"));
    let fixed_bits: Option<usize> = bits_attr.and_then(|attr| {
        attr.parse_args::<syn::LitInt>().ok().and_then(|lit| lit.base10_parse::<usize>().ok())
    });

    let serialize_body = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => {
                let serialize_fields = fields.named.iter().filter_map(|f| {
                    if should_serialize_field(f, serialize_all) {
                        let name = f.ident.as_ref().unwrap();
                        Some(quote! { writer = self.#name.serialize(writer)?; })
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(writer) }
            }
            Fields::Unnamed(fields) => {
                let serialize_fields = (0..fields.unnamed.len()).filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i], serialize_all) {
                        let index = Index::from(i);
                        Some(quote! { writer = self.#index.serialize(writer)?; })
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(writer) }
            }
            Fields::Unit => quote! { Ok(writer) },
        },
        Data::Enum(data) => {
            let variant_count = data.variants.len();
            let bits = fixed_bits.unwrap_or_else(|| if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize });
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u64;
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect::<Vec<_>>();
                        let serialize_fields = fields.named.iter().filter_map(|f| {
                            if should_serialize_field(f, serialize_all) {
                                let name = f.ident.as_ref().unwrap();
                                Some(quote! { writer = #name.serialize(writer)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name { #(#field_names),* } => {
                                writer = writer.write_bits(#variant_index, #bits)?;
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names = (0..fields.unnamed.len())
                            .map(|i| syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                            .collect::<Vec<_>>();
                        let serialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                            if should_serialize_field(f, serialize_all) {
                                let name = &field_names[i];
                                Some(quote! { writer = #name.serialize(writer)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name(#(#field_names),*) => {
                                writer = writer.write_bits(#variant_index, #bits)?;
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        Self::#variant_name => {
                            writer = writer.write_bits(#variant_index, #bits)?;
                            Ok(writer)
                        }
                    },
                }
            });
            quote! { match self { #(#variants),* } }
        },
        Data::Union(_) => panic!("Unions are not supported for Serialize"),
    };

    let expanded = quote! {
        impl #impl_generics Serialize for #name #ty_generics #where_clause {
            fn serialize(&self, mut writer: crate::bit_io::BitWriter) -> ::std::io::Result<crate::bit_io::BitWriter> {
                #serialize_body
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Deserialize, attributes(serialize_all, serialize, no_serialize, bits))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { Deserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));
    let bits_attr = input.attrs.iter().find(|attr| attr.path().is_ident("bits"));
    let fixed_bits: Option<usize> = bits_attr.and_then(|attr| {
        attr.parse_args::<syn::LitInt>().ok().and_then(|lit| lit.base10_parse::<usize>().ok())
    });

    let deserialize_body = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => {
                let field_names = fields.named.iter().filter_map(|f| {
                    if should_serialize_field(f, serialize_all) {
                        f.ident.as_ref().map(|ident| ident.clone())
                    } else {
                        None
                    }
                }).collect::<Vec<_>>();
                let field_defaults = fields.named.iter().filter_map(|f| {
                    if !should_serialize_field(f, serialize_all) {
                        f.ident.as_ref().map(|ident| quote! { #ident: Default::default() })
                    } else {
                        None
                    }
                });
                let deserialize_fields = fields.named.iter().filter_map(|f| {
                    if should_serialize_field(f, serialize_all) {
                        let name = f.ident.as_ref().unwrap();
                        Some(quote! { let (#name, reader) = Deserialize::deserialize(reader)?; })
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok((Self { #(#field_names,)* #(#field_defaults),* }, reader))
                }
            }
            Fields::Unnamed(fields) => {
                let field_names = (0..fields.unnamed.len())
                    .filter_map(|i| {
                        if should_serialize_field(&fields.unnamed[i], serialize_all) {
                            Some(syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let field_defaults = (0..fields.unnamed.len())
                    .filter_map(|i| {
                        if !should_serialize_field(&fields.unnamed[i], serialize_all) {
                            Some(quote! { Default::default() })
                        } else {
                            None
                        }
                    });
                let deserialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                    if should_serialize_field(f, serialize_all) {
                        let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                        Some(quote! { let (#name, reader) = Deserialize::deserialize(reader)?; })
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok((Self(#(#field_names,)* #(#field_defaults),*), reader))
                }
            }
            Fields::Unit => quote! { Ok((Self, reader)) },
        },
        Data::Enum(data) => {
            let variant_count = data.variants.len();
            let bits = fixed_bits.unwrap_or_else(|| if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize });
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u64;
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names = fields.named.iter().filter_map(|f| {
                            if should_serialize_field(f, serialize_all) {
                                f.ident.as_ref().map(|ident| ident.clone())
                            } else {
                                None
                            }
                        }).collect::<Vec<_>>();
                        let field_defaults = fields.named.iter().filter_map(|f| {
                            if !should_serialize_field(f, serialize_all) {
                                f.ident.as_ref().map(|ident| quote! { #ident: Default::default() })
                            } else {
                                None
                            }
                        });
                        let deserialize_fields = fields.named.iter().filter_map(|f| {
                            if should_serialize_field(f, serialize_all) {
                                let name = f.ident.as_ref().unwrap();
                                Some(quote! { let (#name, reader) = Deserialize::deserialize(reader)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok((Self::#variant_name { #(#field_names,)* #(#field_defaults),* }, reader))
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names = (0..fields.unnamed.len())
                            .filter_map(|i| {
                                if should_serialize_field(&fields.unnamed[i], serialize_all) {
                                    Some(syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                                } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                        let field_defaults = (0..fields.unnamed.len())
                            .filter_map(|i| {
                                if !should_serialize_field(&fields.unnamed[i], serialize_all) {
                                    Some(quote! { Default::default() })
                                } else {
                                    None
                                }
                            });
                        let deserialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                            if should_serialize_field(f, serialize_all) {
                                let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                                Some(quote! { let (#name, reader) = Deserialize::deserialize(reader)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok((Self::#variant_name(#(#field_names,)* #(#field_defaults),*), reader))
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        #variant_index => Ok((Self::#variant_name, reader))
                    },
                }
            });
            quote! {
                let (variant_index, reader) = reader.read_bits(#bits)?;
                match variant_index {
                    #(#variants),*
                    _ => Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData, "Unknown variant index")),
                }
            }
        },
        Data::Union(_) => panic!("Unions are not supported for Deserialize"),
    };

    let expanded = quote! {
        impl #impl_generics Deserialize for #name #ty_generics #where_clause {
            fn deserialize(mut reader: crate::bit_io::BitReader) -> ::std::io::Result<(Self, crate::bit_io::BitReader)> {
                #deserialize_body
            }
        }
    };

    TokenStream::from(expanded)
}

fn add_trait_bounds(mut generics: Generics, bound: proc_macro2::TokenStream) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(syn::parse2(bound.clone()).unwrap());
        }
    }
    generics
}

fn should_serialize_field(field: &Field, serialize_all: bool) -> bool {
    let has_serialize = field.attrs.iter().any(|attr| attr.path().is_ident("serialize"));
    let has_no_serialize = field.attrs.iter().any(|attr| attr.path().is_ident("no_serialize"));
    
    if has_no_serialize {
        return false;
    }
    if has_serialize {
        return true;
    }
    serialize_all
}