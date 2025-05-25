use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Index, GenericParam, Generics, Field, Type};

fn add_trait_bounds(mut generics: Generics, bound: proc_macro2::TokenStream) -> Generics {
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
    field.attrs.iter()
        .find(|attr| attr.path().is_ident("bits"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(syn::MetaNameValue {
                    value: syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(lit),
                        ..
                    }),
                    ..
                }) => lit.base10_parse::<usize>().ok(),
                _ => None,
            }
        })
}

fn get_max_len(field: &Field, input: &DeriveInput) -> Option<usize> {
    let field_max_len = field.attrs.iter()
        .find(|attr| attr.path().is_ident("max_len"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(syn::MetaNameValue {
                    value: syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(lit),
                        ..
                    }),
                    ..
                }) => {
                    let result = lit.base10_parse::<usize>().ok();
                    eprintln!("Field max_len for {:?}: {:?}", field.ident, result);
                    result
                }
                _ => {
                    eprintln!("Field max_len parse failed for {:?}", field.ident);
                    None
                }
            }
        });

    if field_max_len.is_none() {
        let default_max_len = input.attrs.iter()
            .find(|attr| attr.path().is_ident("default_max_len"))
            .and_then(|attr| {
                match &attr.meta {
                    syn::Meta::NameValue(syn::MetaNameValue {
                        value: syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Int(lit),
                            ..
                        }),
                        ..
                    }) => {
                        let result = lit.base10_parse::<usize>().ok();
                        eprintln!("Default max_len for input: {:?}", result);
                        result
                    }
                    _ => {
                        eprintln!("Default max_len parse failed");
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
    input.attrs.iter()
        .filter(|attr| attr.path().is_ident("default_bits"))
        .flat_map(|attr| {
            attr.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
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
            Some("u8") | Some("i8") => 8, // Use full 8 bits for u8
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
                Some("bool") if bits != 1 => Err(syn::Error::new_spanned(&field.ty, "Bool requires exactly 1 bit")),
                Some("u8") | Some("i8") if bits > 8 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u8/i8 capacity")),
                Some("u16") | Some("i16") if bits > 16 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u16/i16 capacity")),
                Some("u32") | Some("i32") if bits > 32 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u32/i32 capacity")),
                Some("u64") | Some("i64") if bits > 64 => Err(syn::Error::new_spanned(&field.ty, "Bits exceed u64/i64 capacity")),
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

fn get_enum_bits(input: &DeriveInput) -> Option<usize> {
    input.attrs.iter()
        .find(|attr| attr.path().is_ident("bits"))
        .and_then(|attr| {
            match &attr.meta {
                syn::Meta::NameValue(syn::MetaNameValue {
                    value: syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(lit),
                        ..
                    }),
                    ..
                }) => lit.base10_parse::<usize>().ok(),
                _ => None,
            }
        })
}

#[proc_macro_derive(NetworkSerialize, attributes(no_serialize, bits, max_len, byte_align, default_bits, default_max_len))]
pub fn derive_network_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let bit_serialize_impl = generate_bit_serialize_impl(&input, name);
    let bit_deserialize_impl = generate_bit_deserialize_impl(&input, name);
    let byte_aligned_serialize_impl = generate_byte_aligned_serialize_impl(&input, name);
    let byte_aligned_deserialize_impl = generate_byte_aligned_deserialize_impl(&input, name);

    let expanded = quote! {
        #bit_serialize_impl
        #bit_deserialize_impl
        #byte_aligned_serialize_impl
        #byte_aligned_deserialize_impl
    };

    TokenStream::from(expanded)
}

fn generate_bit_serialize_impl(input: &DeriveInput, name: &syn::Ident) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(input.generics.clone(), quote! { crate::serialize::BitSerialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_body = match &input.data {
        Data::Struct(data) => generate_struct_serialize(&data.fields, true, input),
        Data::Enum(data) => generate_enum_serialize(data, true, input),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    quote! {
        impl #impl_generics crate::serialize::BitSerialize for #name #ty_generics #where_clause {
            fn bit_serialize<W: crate::serialize::bit_io::BitWrite>(&self, writer: &mut W) -> std::io::Result<()> {
                #serialize_body
            }
        }
    }
}

fn generate_bit_deserialize_impl(input: &DeriveInput, name: &syn::Ident) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(input.generics.clone(), quote! { crate::serialize::BitDeserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let deserialize_body = match &input.data {
        Data::Struct(data) => generate_struct_deserialize(&data.fields, true, input),
        Data::Enum(data) => generate_enum_deserialize(data, true, input),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    quote! {
        impl #impl_generics crate::serialize::BitDeserialize for #name #ty_generics #where_clause {
            fn bit_deserialize<R: crate::serialize::bit_io::BitRead>(reader: &mut R) -> std::io::Result<Self> {
                #deserialize_body
            }
        }
    }
}

fn generate_byte_aligned_serialize_impl(input: &DeriveInput, name: &syn::Ident) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(input.generics.clone(), quote! { crate::serialize::ByteAlignedSerialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let serialize_body = match &input.data {
        Data::Struct(data) => generate_struct_serialize(&data.fields, false, input),
        Data::Enum(data) => generate_enum_serialize(data, false, input),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    quote! {
        impl #impl_generics crate::serialize::ByteAlignedSerialize for #name #ty_generics #where_clause {
            fn byte_aligned_serialize<W: std::io::Write + byteorder::WriteBytesExt>(&self, writer: &mut W) -> std::io::Result<()> {
                #serialize_body
            }
        }
    }
}

fn generate_byte_aligned_deserialize_impl(input: &DeriveInput, name: &syn::Ident) -> proc_macro2::TokenStream {
    let generics = add_trait_bounds(input.generics.clone(), quote! { crate::serialize::ByteAlignedDeserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let deserialize_body = match &input.data {
        Data::Struct(data) => generate_struct_deserialize(&data.fields, false, input),
        Data::Enum(data) => generate_enum_deserialize(data, false, input),
        Data::Union(_) => panic!("Unions are not supported"),
    };

    quote! {
        impl #impl_generics crate::serialize::ByteAlignedDeserialize for #name #ty_generics #where_clause {
            fn byte_aligned_deserialize<R: std::io::Read + byteorder::ReadBytesExt>(reader: &mut R) -> std::io::Result<Self> {
                #deserialize_body
            }
        }
    }
}

fn generate_struct_serialize(fields: &Fields, is_bit: bool, input: &DeriveInput) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    match fields {
        Fields::Named(fields) => {
            let serialize_fields = fields.named.iter().filter_map(|f| {
                let name = f.ident.as_ref().unwrap();
                if should_serialize_field(f) {
                    let is_byte_align = is_byte_aligned(f);
                    let bits = get_field_bit_width(f, &defaults);
                    let max_len = get_max_len(f, input);
                    let value_expr = quote! { self.#name };
                    let serialize_code = if is_bit {
                        if bits > 0 {
                            quote! {
                                if #value_expr as u64 > (1u64 << #bits) - 1 {
                                    return Err(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        format!("Value {} exceeds {} bits for field {:?}", #value_expr, #bits, stringify!(#name))
                                    ));
                                }
                                writer.write_bits(#value_expr as u64, #bits)?;
                            }
                        } else if is_vec_type(&f.ty) {
                            let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                (len_bits, quote! { #max_len })
                            } else {
                                let default_len_bits = 16usize;
                                (default_len_bits, quote! { 65535usize })
                            };
                            quote! {
                                let max_len = #max_len_expr;
                                if self.#name.len() > max_len {
                                    log::debug!("Vector length {} exceeds max_len {} for field {:?}", self.#name.len(), max_len, stringify!(#name));
                                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", self.#name.len(), max_len)));
                                }
                                writer.write_bits(self.#name.len() as u64, #len_bits)?;
                                for item in &self.#name {
                                    item.bit_serialize(writer)?;
                                }
                            }
                        } else {
                            quote! { self.#name.bit_serialize(writer)?; }
                        }
                    } else {
                        quote! { self.#name.byte_aligned_serialize(writer)?; }
                    };
                    if is_byte_align && is_bit {
                        Some(quote! {
                            while writer.bit_pos() % 8 != 0 {
                                writer.write_bit(false)?;
                            }
                            #serialize_code
                        })
                    } else {
                        Some(serialize_code)
                    }
                } else {
                    None
                }
            });
            quote! { #(#serialize_fields)* Ok(()) }
        }
        Fields::Unnamed(fields) => {
            let serialize_fields = (0..fields.unnamed.len()).filter_map(|i| {
                if should_serialize_field(&fields.unnamed[i]) {
                    let index = Index::from(i);
                    let is_byte_align = is_byte_aligned(&fields.unnamed[i]);
                    let bits = get_field_bit_width(&fields.unnamed[i], &defaults);
                    let max_len = get_max_len(&fields.unnamed[i], input);
                    let value_expr = quote! { self.#index };
                    let serialize_code = if is_bit {
                        if bits > 0 {
                            quote! {
                                if #value_expr as u64 > (1u64 << #bits) - 1 {
                                    return Err(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        format!("Value {} exceeds {} bits for field {}", #value_expr, #bits, #index)
                                    ));
                                }
                                writer.write_bits(#value_expr as u64, #bits)?;
                            }
                        } else if is_vec_type(&fields.unnamed[i].ty) {
                            let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                (len_bits, quote! { #max_len })
                            } else {
                                let default_len_bits = 16usize;
                                (default_len_bits, quote! { 65535usize })
                            };
                            quote! {
                                let max_len = #max_len_expr;
                                if self.#index.len() > max_len {
                                    log::debug!("Vector length {} exceeds max_len {} for field {}", self.#index.len(), max_len, #index);
                                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", self.#index.len(), max_len)));
                                }
                                writer.write_bits(self.#index.len() as u64, #len_bits)?;
                                for item in &self.#index {
                                    item.bit_serialize(writer)?;
                                }
                            }
                        } else {
                            quote! { self.#index.bit_serialize(writer)?; }
                        }
                    } else {
                        quote! { self.#index.byte_aligned_serialize(writer)?; }
                    };
                    if is_byte_align && is_bit {
                        Some(quote! {
                            while writer.bit_pos() % 8 != 0 {
                                writer.write_bit(false)?;
                            }
                            #serialize_code
                        })
                    } else {
                        Some(serialize_code)
                    }
                } else {
                    None
                }
            });
            quote! { #(#serialize_fields)* Ok(()) }
        }
        Fields::Unit => quote! { Ok(()) },
    }
}

fn generate_struct_deserialize(fields: &Fields, is_bit: bool, input: &DeriveInput) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    match fields {
        Fields::Named(fields) => {
            let field_names = fields.named.iter().filter_map(|f| {
                if should_serialize_field(f) {
                    f.ident.as_ref().map(|ident| ident.clone())
                } else {
                    None
                }
            }).collect::<Vec<_>>();
            let field_defaults = fields.named.iter().filter_map(|f| {
                if !should_serialize_field(f) {
                    f.ident.as_ref().map(|ident| quote! { #ident: Default::default() })
                } else {
                    None
                }
            });
            let deserialize_fields = fields.named.iter().filter_map(|f| {
                let name = f.ident.as_ref().unwrap();
                if should_serialize_field(f) {
                    let is_byte_align = is_byte_aligned(f);
                    let bits = get_field_bit_width(f, &defaults);
                    let max_len = get_max_len(f, input);
                    let type_name = match &f.ty {
                        Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                        _ => None,
                    };
                    let deserialize_code = if is_bit {
                        if bits > 0 {
                            if type_name.as_deref() == Some("bool") {
                                quote! { let #name = reader.read_bits(#bits)? != 0; }
                            } else {
                                quote! { let #name = reader.read_bits(#bits)? as _; }
                            }
                        } else if is_vec_type(&f.ty) {
                            let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                (len_bits, quote! { #max_len })
                            } else {
                                let default_len_bits = 16usize;
                                (default_len_bits, quote! { 65535usize })
                            };
                            quote! {
                                let len = reader.read_bits(#len_bits)? as usize;
                                if len > #max_len_expr {
                                    log::debug!("Vector length {} exceeds max_len {} for field {:?}", len, #max_len_expr, stringify!(#name));
                                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", len, #max_len_expr)));
                                }
                                let mut #name = Vec::with_capacity(len);
                                for _ in 0..len {
                                    #name.push(crate::serialize::BitDeserialize::bit_deserialize(reader)?);
                                }
                            }
                        } else {
                            quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; }
                        }
                    } else {
                        quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; }
                    };
                    if is_byte_align && is_bit {
                        Some(quote! {
                            while reader.bit_pos() % 8 != 0 {
                                reader.read_bit()?;
                            }
                            #deserialize_code
                        })
                    } else {
                        Some(deserialize_code)
                    }
                } else {
                    None
                }
            });
            quote! {
                #(#deserialize_fields)*
                Ok(Self { #(#field_names,)* #(#field_defaults,)* })
            }
        }
        Fields::Unnamed(fields) => {
            let field_names = (0..fields.unnamed.len())
                .filter_map(|i| {
                    if should_serialize_field(&fields.unnamed[i]) {
                        Some(syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let field_defaults = (0..fields.unnamed.len())
                .filter_map(|i| {
                    if !should_serialize_field(&fields.unnamed[i]) {
                        Some(quote! { Default::default() })
                    } else {
                        None
                    }
                });
            let deserialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                if should_serialize_field(f) {
                    let is_byte_align = is_byte_aligned(f);
                    let bits = get_field_bit_width(f, &defaults);
                    let max_len = get_max_len(f, input);
                    let type_name = match &f.ty {
                        Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                        _ => None,
                    };
                    let deserialize_code = if is_bit {
                        if bits > 0 {
                            if type_name.as_deref() == Some("bool") {
                                quote! { let #name = reader.read_bits(#bits)? != 0; }
                            } else {
                                quote! { let #name = reader.read_bits(#bits)? as _; }
                            }
                        } else if is_vec_type(&f.ty) {
                            let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                (len_bits, quote! { #max_len })
                            } else {
                                let default_len_bits = 16usize;
                                (default_len_bits, quote! { 65535usize })
                            };
                            quote! {
                                let len = reader.read_bits(#len_bits)? as usize;
                                if len > #max_len_expr {
                                    log::debug!("Vector length {} exceeds max_len {} for field {}", len, #max_len_expr, #i);
                                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", len, #max_len_expr)));
                                }
                                let mut #name = Vec::with_capacity(len);
                                for _ in 0..len {
                                    #name.push(crate::serialize::BitDeserialize::bit_deserialize(reader)?);
                                }
                            }
                        } else {
                            quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; }
                        }
                    } else {
                        quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; }
                    };
                    if is_byte_align && is_bit {
                        Some(quote! {
                            while reader.bit_pos() % 8 != 0 {
                                reader.read_bit()?;
                            }
                            #deserialize_code
                        })
                    } else {
                        Some(deserialize_code)
                    }
                } else {
                    None
                }
            });
            quote! {
                #(#deserialize_fields)*
                Ok(Self(#(#field_names,)* #(#field_defaults,)*))
            }
        }
        Fields::Unit => quote! { Ok(Self) },
    }
}

fn generate_enum_serialize(data: &syn::DataEnum, is_bit: bool, input: &DeriveInput) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    let variant_count = data.variants.len();
    let min_bits = if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize };
    let bits = get_enum_bits(input).unwrap_or(min_bits);

    if bits < min_bits {
        panic!("Enum bits attribute ({}) too small to represent {} variants (needs at least {})", bits, variant_count, min_bits);
    }
    if bits > 64 {
        panic!("Enum bits attribute ({}) exceeds 64, too large for variant index", bits);
    }
    if !is_bit && variant_count > 256 {
        panic!("Too many enum variants ({}) for byte-aligned serialization (max 256)", variant_count);
    }

    let variants = data.variants.iter().enumerate().map(|(i, variant)| {
        let variant_name = &variant.ident;
        let variant_index = i as u64;
        let serialize_code = if is_bit {
            quote! { writer.write_bits(#variant_index, #bits)?; }
        } else {
            quote! { writer.write_u8(#variant_index as u8)?; }
        };
        match &variant.fields {
            Fields::Named(fields) => {
                let field_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect::<Vec<_>>();
                let serialize_fields = fields.named.iter().filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let serialize_code = if is_bit {
                            if bits > 0 {
                                quote! {
                                    if *#name as u64 > (1u64 << #bits) - 1 {
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            format!("Value {} exceeds {} bits for field {:?}", *#name, #bits, stringify!(#name))
                                        ));
                                    }
                                    writer.write_bits(*#name as u64, #bits)?;
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                    (len_bits, quote! { #max_len })
                                } else {
                                    let default_len_bits = 16usize;
                                    (default_len_bits, quote! { 65535usize })
                                };
                                quote! {
                                    let max_len = #max_len_expr;
                                    if #name.len() > max_len {
                                        log::debug!("Vector length {} exceeds max_len {} for field {:?}", #name.len(), max_len, stringify!(#name));
                                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", #name.len(), max_len)));
                                    }
                                    writer.write_bits(#name.len() as u64, #len_bits)?;
                                    for item in #name {
                                        item.bit_serialize(writer)?;
                                    }
                                }
                            } else {
                                quote! { #name.bit_serialize(writer)?; }
                            }
                        } else {
                            if bits > 0 {
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                                    _ => None,
                                };
                                match type_name.as_deref() {
                                    Some("u8") | Some("i8") => quote! { writer.write_u8(*#name)?; },
                                    Some("u16") | Some("i16") => quote! { writer.write_u16::<byteorder::LittleEndian>(*#name as u16)?; },
                                    Some("u32") | Some("i32") => quote! { writer.write_u32::<byteorder::LittleEndian>(*#name as u32)?; },
                                    Some("u64") | Some("i64") => quote! { writer.write_u64::<byteorder::LittleEndian>(*#name as u64)?; },
                                    Some("bool") => quote! { writer.write_u8(if *#name { 1 } else { 0 })?; },
                                    _ => quote! { #name.byte_aligned_serialize(writer)?; },
                                }
                            } else {
                                quote! { #name.byte_aligned_serialize(writer)?; }
                            }
                        };
                        if is_byte_align && is_bit {
                            Some(quote! {
                                while writer.bit_pos() % 8 != 0 {
                                    writer.write_bit(false)?;
                                }
                                #serialize_code
                            })
                        } else {
                            Some(serialize_code)
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    Self::#variant_name { #(#field_names),* } => {
                        #serialize_code
                        #(#serialize_fields)*
                        Ok(())
                    },
                }
            }
            Fields::Unnamed(fields) => {
                let field_names = (0..fields.unnamed.len())
                    .map(|i| syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                    .collect::<Vec<_>>();
                let serialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                    let name = &field_names[i];
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let serialize_code = if is_bit {
                            if bits > 0 {
                                quote! {
                                    if *#name as u64 > (1u64 << #bits) - 1 {
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::InvalidData,
                                            format!("Value {} exceeds {} bits for field {}", *#name, #bits, #i)
                                        ));
                                    }
                                    writer.write_bits(*#name as u64, #bits)?;
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                    (len_bits, quote! { #max_len })
                                } else {
                                    let default_len_bits = 16usize;
                                    (default_len_bits, quote! { 65535usize })
                                };
                                quote! {
                                    let max_len = #max_len_expr;
                                    if #name.len() > max_len {
                                        log::debug!("Vector length {} exceeds max_len {} for field {}", #name.len(), max_len, #i);
                                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", #name.len(), max_len)));
                                    }
                                    writer.write_bits(#name.len() as u64, #len_bits)?;
                                    for item in #name {
                                        item.bit_serialize(writer)?;
                                    }
                                }
                            } else {
                                quote! { #name.bit_serialize(writer)?; }
                            }
                        } else {
                            if bits > 0 {
                                let type_name = match &f.ty {
                                    Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                                    _ => None,
                                };
                                match type_name.as_deref() {
                                    Some("u8") | Some("i8") => quote! { writer.write_u8(*#name)?; },
                                    Some("u16") | Some("i16") => quote! { writer.write_u16::<byteorder::LittleEndian>(*#name as u16)?; },
                                    Some("u32") | Some("i32") => quote! { writer.write_u32::<byteorder::LittleEndian>(*#name as u32)?; },
                                    Some("u64") | Some("i64") => quote! { writer.write_u64::<byteorder::LittleEndian>(*#name as u64)?; },
                                    Some("bool") => quote! { writer.write_u8(if *#name { 1 } else { 0 })?; },
                                    _ => quote! { #name.byte_aligned_serialize(writer)?; },
                                }
                            } else {
                                quote! { #name.byte_aligned_serialize(writer)?; }
                            }
                        };
                        if is_byte_align && is_bit {
                            Some(quote! {
                                while writer.bit_pos() % 8 != 0 {
                                    writer.write_bit(false)?;
                                }
                                #serialize_code
                            })
                        } else {
                            Some(serialize_code)
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    Self::#variant_name(#(#field_names),*) => {
                        #serialize_code
                        #(#serialize_fields)*
                        Ok(())
                    },
                }
            }
            Fields::Unit => quote! {
                Self::#variant_name => {
                    #serialize_code
                    Ok(())
                },
            },
        }
    });

    quote! { 
        match self { 
            #(#variants)* 
        } 
    }
}

fn generate_enum_deserialize(data: &syn::DataEnum, is_bit: bool, input: &DeriveInput) -> proc_macro2::TokenStream {
    let defaults = get_default_bits(input);
    let variant_count = data.variants.len();
    let min_bits = if variant_count == 0 { 0 } else { (variant_count as f64).log2().ceil() as usize };
    let bits = get_enum_bits(input).unwrap_or(min_bits);

    if bits < min_bits {
        panic!("Enum bits attribute ({}) too small to represent {} variants (needs at least {})", bits, variant_count, min_bits);
    }
    if bits > 64 {
        panic!("Enum bits attribute ({}) exceeds 64, too large for variant index", bits);
    }
    if !is_bit && variant_count > 256 {
        panic!("Too many enum variants ({}) for byte-aligned serialization (max 256)", variant_count);
    }

    let variants = data.variants.iter().enumerate().map(|(i, variant)| {
        let variant_name = &variant.ident;
        let variant_index = i as u64;
        match &variant.fields {
            Fields::Named(fields) => {
                let field_names = fields.named.iter().filter_map(|f| {
                    if should_serialize_field(f) {
                        f.ident.as_ref().map(|ident| ident.clone())
                    } else {
                        None
                    }
                }).collect::<Vec<_>>();
                let field_defaults = fields.named.iter().filter_map(|f| {
                    if !should_serialize_field(f) {
                        f.ident.as_ref().map(|ident| quote! { #ident: Default::default() })
                    } else {
                        None
                    }
                });
                let deserialize_fields = fields.named.iter().filter_map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let type_name = match &f.ty {
                            Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                            _ => None,
                        };
                        let deserialize_code = if is_bit {
                            if bits > 0 {
                                if type_name.as_deref() == Some("bool") {
                                    quote! { let #name = reader.read_bits(#bits)? != 0; }
                                } else {
                                    quote! { let #name = reader.read_bits(#bits)? as _; }
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                    (len_bits, quote! { #max_len })
                                } else {
                                    let default_len_bits = 16usize;
                                    (default_len_bits, quote! { 65535usize })
                                };
                                quote! {
                                    let len = reader.read_bits(#len_bits)? as usize;
                                    if len > #max_len_expr {
                                        log::debug!("Vector length {} exceeds max_len {} for field {:?}", len, #max_len_expr, stringify!(#name));
                                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", len, #max_len_expr)));
                                    }
                                    let mut #name = Vec::with_capacity(len);
                                    for _ in 0..len {
                                        #name.push(crate::serialize::BitDeserialize::bit_deserialize(reader)?);
                                    }
                                }
                            } else {
                                quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; }
                            }
                        } else {
                            if bits > 0 {
                                match type_name.as_deref() {
                                    Some("u8") | Some("i8") => quote! { let #name = reader.read_u8()?; },
                                    Some("u16") | Some("i16") => quote! { let #name = reader.read_u16::<byteorder::LittleEndian>()? as _; },
                                    Some("u32") | Some("i32") => quote! { let #name = reader.read_u32::<byteorder::LittleEndian>()? as _; },
                                    Some("u64") | Some("i64") => quote! { let #name = reader.read_u64::<byteorder::LittleEndian>()? as _; },
                                    Some("bool") => quote! { let #name = reader.read_u8()? != 0; },
                                    _ => quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; },
                                }
                            } else {
                                quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; }
                            }
                        };
                        if is_byte_align && is_bit {
                            Some(quote! {
                                while reader.bit_pos() % 8 != 0 {
                                    reader.read_bit()?;
                                }
                                #deserialize_code
                            })
                        } else {
                            Some(deserialize_code)
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    #variant_index => {
                        #(#deserialize_fields)*
                        Ok(Self::#variant_name { #(#field_names,)* #(#field_defaults,)* })
                    },
                }
            }
            Fields::Unnamed(fields) => {
                let field_names = (0..fields.unnamed.len())
                    .filter_map(|i| {
                        if should_serialize_field(&fields.unnamed[i]) {
                            Some(syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site()))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let field_defaults = (0..fields.unnamed.len())
                    .filter_map(|i| {
                        if !should_serialize_field(&fields.unnamed[i]) {
                            Some(quote! { Default::default() })
                        } else {
                            None
                        }
                    });
                let deserialize_fields = fields.unnamed.iter().enumerate().filter_map(|(i, f)| {
                    let name = syn::Ident::new(&format!("field_{i}"), proc_macro2::Span::call_site());
                    if should_serialize_field(f) {
                        let is_byte_align = is_byte_aligned(f);
                        let bits = get_field_bit_width(f, &defaults);
                        let max_len = get_max_len(f, input);
                        let type_name = match &f.ty {
                            Type::Path(type_path) => type_path.path.get_ident().map(|i| i.to_string()),
                            _ => None,
                        };
                        let deserialize_code = if is_bit {
                            if bits > 0 {
                                if type_name.as_deref() == Some("bool") {
                                    quote! { let #name = reader.read_bits(#bits)? != 0; }
                                } else {
                                    quote! { let #name = reader.read_bits(#bits)? as _; }
                                }
                            } else if is_vec_type(&f.ty) {
                                let (len_bits, max_len_expr) = if let Some(max_len) = max_len {
                                    let len_bits = ((max_len + 1) as f64).log2().ceil() as usize;
                                    (len_bits, quote! { #max_len })
                                } else {
                                    let default_len_bits = 16usize;
                                    (default_len_bits, quote! { 65535usize })
                                };
                                quote! {
                                    let len = reader.read_bits(#len_bits)? as usize;
                                    if len > #max_len_expr {
                                        log::debug!("Vector length {} exceeds max_len {} for field {}", len, #max_len_expr, #i);
                                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Vector length {} exceeds max_len {}", len, #max_len_expr)));
                                    }
                                    let mut #name = Vec::with_capacity(len);
                                    for _ in 0..len {
                                        #name.push(crate::serialize::BitDeserialize::bit_deserialize(reader)?);
                                    }
                                }
                            } else {
                                quote! { let #name = crate::serialize::BitDeserialize::bit_deserialize(reader)?; }
                            }
                        } else {
                            if bits > 0 {
                                match type_name.as_deref() {
                                    Some("u8") | Some("i8") => quote! { let #name = reader.read_u8()?; },
                                    Some("u16") | Some("i16") => quote! { let #name = reader.read_u16::<byteorder::LittleEndian>()? as _; },
                                    Some("u32") | Some("i32") => quote! { let #name = reader.read_u32::<byteorder::LittleEndian>()? as _; },
                                    Some("u64") | Some("i64") => quote! { let #name = reader.read_u64::<byteorder::LittleEndian>()? as _; },
                                    Some("bool") => quote! { let #name = reader.read_u8()? != 0; },
                                    _ => quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; },
                                }
                            } else {
                                quote! { let #name = crate::serialize::ByteAlignedDeserialize::byte_aligned_deserialize(reader)?; }
                            }
                        };
                        if is_byte_align && is_bit {
                            Some(quote! {
                                while reader.bit_pos() % 8 != 0 {
                                    reader.read_bit()?;
                                }
                                #deserialize_code
                            })
                        } else {
                            Some(deserialize_code)
                        }
                    } else {
                        None
                    }
                });
                quote! {
                    #variant_index => {
                        #(#deserialize_fields)*
                        Ok(Self::#variant_name(#(#field_names,)* #(#field_defaults,)*))
                    },
                }
            }
            Fields::Unit => quote! {
                #variant_index => Ok(Self::#variant_name),
            }
        }
    });

    if is_bit {
        quote! {
            let variant_index = reader.read_bits(#bits)?;
            match variant_index {
                #(#variants)*
                _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown variant index")),
            }
        }
    } else {
        quote! {
            let variant_index = reader.read_u8()? as u64;
            match variant_index {
                #(#variants)*
                _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown variant index")),
            }
        }
    }
}