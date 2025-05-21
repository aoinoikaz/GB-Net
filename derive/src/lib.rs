use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Index, GenericParam, Generics, Field, LitInt, Type};

#[proc_macro_derive(Serialize, attributes(serialize_all, no_serialize))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { Serialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));

    let serialize_body = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => {
                let serialize_fields = fields.named.iter().filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f, serialize_all) {
                        Some(quote! { self.#name.serialize(writer)?; })
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(()) }
            }
            Fields::Unnamed(fields) => {
                let serialize_fields = (0..fields.unnamed.len()).filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i], serialize_all) {
                        let index = Index::from(i);
                        Some(quote! { self.#index.serialize(writer)?; })
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(()) }
            }
            Fields::Unit => quote! { Ok(()) },
        },
        Data::Enum(data) => {
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u8;
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect::<Vec<_>>();
                        let serialize_fields = fields.named.iter().filter_map(|f| {
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f, serialize_all) {
                                Some(quote! { #name.serialize(writer)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name { #(#field_names),* } => {
                                writer.write_u8(#variant_index)?;
                                #(#serialize_fields)*
                                Ok(())
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names = (0..fields.unnamed.len())
                            .map(|i| syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                            .collect::<Vec<_>>();
                        let serialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                            let name = &field_names[i];
                            if should_serialize_field(f, serialize_all) {
                                Some(quote! { #name.serialize(writer)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name(#(#field_names),*) => {
                                writer.write_u8(#variant_index)?;
                                #(#serialize_fields)*
                                Ok(())
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        Self::#variant_name => {
                            writer.write_u8(#variant_index)?;
                            Ok(())
                        }
                    },
                }
            });
            quote! { match self { #(#variants),* } }
        },
        Data::Union(_) => panic!("Unions are not supported for Serialize"),
    };

    let expanded = quote! {
        impl #impl_generics crate::serialize::Serialize for #name #ty_generics #where_clause {
            fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                #serialize_body
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(Deserialize, attributes(serialize_all, no_serialize))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { Deserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));

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
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f, serialize_all) {
                        Some(quote! { let #name = crate::serialize::Deserialize::deserialize(reader)?; })
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok(Self { #(#field_names,)* #(#field_defaults),* })
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
                    let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                    if should_serialize_field(f, serialize_all) {
                        Some(quote! { let #name = crate::serialize::Deserialize::deserialize(reader)?; })
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok(Self(#(#field_names,)* #(#field_defaults),*))
                }
            }
            Fields::Unit => quote! { Ok(Self) },
        },
        Data::Enum(data) => {
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u8;
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
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f, serialize_all) {
                                Some(quote! { let #name = crate::serialize::Deserialize::deserialize(reader)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok(Self::#variant_name { #(#field_names,)* #(#field_defaults),* })
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
                            let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                            if should_serialize_field(f, serialize_all) {
                                Some(quote! { let #name = crate::serialize::Deserialize::deserialize(reader)?; })
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok(Self::#variant_name(#(#field_names,)* #(#field_defaults),*))
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        #variant_index => Ok(Self::#variant_name)
                    },
                }
            });
            quote! {
                let variant_index = reader.read_u8()?;
                match variant_index {
                    #(#variants),*
                    _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown variant index")),
                }
            }
        },
        Data::Union(_) => panic!("Unions are not supported for Deserialize"),
    };

    let expanded = quote! {
        impl #impl_generics crate::serialize::Deserialize for #name #ty_generics #where_clause {
            fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                #deserialize_body
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(BitSerialize, attributes(serialize_all, no_serialize, bits))]
pub fn derive_bit_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { BitSerialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));
    let bits_attr = input.attrs.iter().find(|attr| attr.path().is_ident("bits"));
    let fixed_bits: Option<usize> = bits_attr.and_then(|attr| {
        attr.parse_args::<LitInt>().ok().and_then(|lit| lit.base10_parse::<usize>().ok())
    });

    let serialize_body = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => {
                let serialize_fields = fields.named.iter().filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f, serialize_all) {
                        if let Some(bits) = get_field_bits(f) {
                            validate_field_bits(f, bits).expect("Invalid bits attribute");
                            Some(quote! { writer.write_bits(self.#name as u64, #bits)?; })
                        } else {
                            Some(quote! { self.#name.bit_serialize(writer)?; })
                        }
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(()) }
            }
            Fields::Unnamed(fields) => {
                let serialize_fields = (0..fields.unnamed.len()).filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i], serialize_all) {
                        let index = Index::from(i);
                        if let Some(bits) = get_field_bits(&fields.unnamed[i]) {
                            validate_field_bits(&fields.unnamed[i], bits).expect("Invalid bits attribute");
                            Some(quote! { writer.write_bits(self.#index as u64, #bits)?; })
                        } else {
                            Some(quote! { self.#index.bit_serialize(writer)?; })
                        }
                    } else {
                        None
                    }
                });
                quote! { #(#serialize_fields)* Ok(()) }
            }
            Fields::Unit => quote! { Ok(()) },
        },
        Data::Enum(data) => {
            let variant_count = data.variants.len();
            let bits = fixed_bits.unwrap_or_else(|| if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize });
            if bits > 8 {
                panic!("Enum variants exceed 256, too many for bit-packing");
            }
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u64;
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect::<Vec<_>>();
                        let serialize_fields = fields.named.iter().filter_map(|f| {
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f, serialize_all) {
                                if let Some(bits) = get_field_bits(f) {
                                    validate_field_bits(f, bits).expect("Invalid bits attribute");
                                    Some(quote! { writer.write_bits(#name as u64, #bits)?; })
                                } else {
                                    Some(quote! { #name.bit_serialize(writer)?; })
                                }
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name { #(#field_names),* } => {
                                writer.write_bits(#variant_index, #bits)?;
                                #(#serialize_fields)*
                                Ok(())
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names = (0..fields.unnamed.len())
                            .map(|i| syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                            .collect::<Vec<_>>();
                        let serialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                            let name = &field_names[i];
                            if should_serialize_field(f, serialize_all) {
                                if let Some(bits) = get_field_bits(f) {
                                    validate_field_bits(f, bits).expect("Invalid bits attribute");
                                    Some(quote! { writer.write_bits(#name as u64, #bits)?; })
                                } else {
                                    Some(quote! { #name.bit_serialize(writer)?; })
                                }
                            } else {
                                None
                            }
                        });
                        quote! {
                            Self::#variant_name(#(#field_names),*) => {
                                writer.write_bits(#variant_index, #bits)?;
                                #(#serialize_fields)*
                                Ok(())
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        Self::#variant_name => {
                            writer.write_bits(#variant_index, #bits)?;
                            Ok(())
                        }
                    },
                }
            });
            quote! { match self { #(#variants),* } }
        },
        Data::Union(_) => panic!("Unions are not supported for BitSerialize"),
    };

    let expanded = quote! {
        impl #impl_generics crate::serialize::BitSerialize for #name #ty_generics #where_clause {
            fn bit_serialize<W: crate::bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                #serialize_body
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(BitDeserialize, attributes(serialize_all, no_serialize, bits))]
pub fn derive_bit_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let generics = add_trait_bounds(input.generics.clone(), quote! { BitDeserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_all = input.attrs.iter().any(|attr| attr.path().is_ident("serialize_all"));
    let bits_attr = input.attrs.iter().find(|attr| attr.path().is_ident("bits"));
    let fixed_bits: Option<usize> = bits_attr.and_then(|attr| {
        attr.parse_args::<LitInt>().ok().and_then(|lit| lit.base10_parse::<usize>().ok())
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
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f, serialize_all) {
                        if let Some(bits) = get_field_bits(f) {
                            validate_field_bits(f, bits).expect("Invalid bits attribute");
                            Some(quote! {
                                let #name = reader.read_bits(#bits)? as _;
                            })
                        } else {
                            Some(quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; })
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok(Self { #(#field_names,)* #(#field_defaults),* })
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
                    let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                    if should_serialize_field(f, serialize_all) {
                        if let Some(bits) = get_field_bits(f) {
                            validate_field_bits(f, bits).expect("Invalid bits attribute");
                            Some(quote! {
                                let #name = reader.read_bits(#bits)? as _;
                            })
                        } else {
                            Some(quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; })
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    #(#deserialize_fields)*
                    Ok(Self(#(#field_names,)* #(#field_defaults),*))
                }
            }
            Fields::Unit => quote! { Ok(Self) },
        },
        Data::Enum(data) => {
            let variant_count = data.variants.len();
            let bits = fixed_bits.unwrap_or_else(|| if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize });
            if bits > 8 {
                panic!("Enum variants exceed 256, too many for bit-packing");
            }
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
                            let name = f.ident.as_ref().unwrap();
                            if should_serialize_field(f, serialize_all) {
                                if let Some(bits) = get_field_bits(f) {
                                    validate_field_bits(f, bits).expect("Invalid bits attribute");
                                    Some(quote! {
                                        let #name = reader.read_bits(#bits)? as _;
                                    })
                                } else {
                                    Some(quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; })
                                }
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok(Self::#variant_name { #(#field_names,)* #(#field_defaults),* })
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
                            let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                            if should_serialize_field(f, serialize_all) {
                                if let Some(bits) = get_field_bits(f) {
                                    validate_field_bits(f, bits).expect("Invalid bits attribute");
                                    Some(quote! {
                                        let #name = reader.read_bits(#bits)? as _;
                                    })
                                } else {
                                    Some(quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; })
                                }
                            } else {
                                None
                            }
                        });
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok(Self::#variant_name(#(#field_names,)* #(#field_defaults),*))
                            }
                        }
                    }
                    Fields::Unit => quote! {
                        #variant_index => Ok(Self::#variant_name)
                    },
                }
            });
            quote! {
                let variant_index = reader.read_bits(#bits)?;
                match variant_index {
                    #(#variants),*
                    _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown variant index")),
                }
            }
        },
        Data::Union(_) => panic!("Unions are not supported for BitDeserialize"),
    };

    let expanded = quote! {
        impl #impl_generics crate::serialize::BitDeserialize for #name #ty_generics #where_clause {
            fn bit_deserialize<R: crate::bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
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
    let has_no_serialize = field.attrs.iter().any(|attr| attr.path().is_ident("no_serialize"));
    !has_no_serialize && serialize_all
}

fn get_field_bits(field: &Field) -> Option<usize> {
    field.attrs.iter().find(|attr| attr.path().is_ident("bits"))
        .and_then(|attr| attr.parse_args::<LitInt>().ok()
        .and_then(|lit| lit.base10_parse::<usize>().ok()))
}

fn validate_field_bits(field: &Field, bits: usize) -> syn::Result<()> {
    if bits > 64 {
        return Err(syn::Error::new_spanned(&field.ty, "Bits attribute exceeds 64"));
    }
    match &field.ty {
        Type::Path(type_path) => {
            let ident = type_path.path.get_ident().map(|i| i.to_string());
            match ident.as_deref() {
                Some("bool") if bits != 1 => Err(syn::Error::new_spanned(&field.ty, "Bool requires exactly 1 bit")),
                Some("u8") if bits > 8 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u8 capacity")),
                Some("u16") if bits > 16 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u16 capacity")),
                Some("u32") if bits > 32 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u32 capacity")),
                Some("i8") if bits > 8 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed i8 capacity")),
                Some("i16") if bits > 16 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed i16 capacity")),
                Some("i32") if bits > 32 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed i32 capacity")),
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}