use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Variant, Index, GenericParam, Generics};

/// Derives the `Serialize` trait for a type, enabling bit-packed serialization.
/// This macro generates code to serialize structs, enums, unions, and generics
/// into a `BitWriter`, ensuring minimal bandwidth usage for networking (Gaffer-style).
#[proc_macro_derive(Serialize)]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    // Parse the input Rust code into a syntax tree (DeriveInput)
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident; // Get the type name (e.g., `Player`)

    // Add trait bounds to generics (e.g., `T: Serialize` for `struct Wrapper<T>`)
    // This ensures all generic types implement Serialize at compile time
    let generics = add_trait_bounds(input.generics.clone(), quote! { ::gbnet::serialize::Serialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate serialization code based on the type (struct, enum, union)
    let expanded = match input.data {
        Data::Struct(data) => {
            match data.fields {
                Fields::Named(fields) => {
                    // Handle structs with named fields: `struct Player { id: u32, name: String }`
                    let field_names = fields.named.iter().map(|f| &f.ident);
                    let serialize_fields = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        // Serialize each field by calling its `serialize` method
                        quote! {
                            writer = self.#name.serialize(writer)?;
                        }
                    });
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Serialize for #name #ty_generics #where_clause {
                            fn serialize(&self, writer: ::gbnet::serialize::BitWriter) -> ::std::io::Result<::gbnet::serialize::BitWriter> {
                                let mut writer = writer;
                                // Serialize each field in order, bit-packing as we go
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                }
                Fields::Unnamed(fields) => {
                    // Handle tuple structs: `struct Point(f32, f32)`
                    let field_indices = (0..fields.unnamed.len()).map(|i| Index::from(i));
                    let serialize_fields = field_indices.clone().map(|i| {
                        // Serialize each tuple element by index
                        quote! {
                            writer = self.#i.serialize(writer)?;
                        }
                    });
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Serialize for #name #ty_generics #where_clause {
                            fn serialize(&self, writer: ::gbnet::serialize::BitWriter) -> ::std::io::Result<::gbnet::serialize::BitWriter> {
                                let mut writer = writer;
                                // Serialize each tuple element in order
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                }
                Fields::Unit => {
                    // Handle unit structs: `struct Empty;`
                    // No fields to serialize, so return the writer as-is
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Serialize for #name #ty_generics #where_clause {
                            fn serialize(&self, writer: ::gbnet::serialize::BitWriter) -> ::std::io::Result<::gbnet::serialize::BitWriter> {
                                Ok(writer)
                            }
                        }
                    }
                }
            }
        }
        Data::Enum(data) => {
            // Handle enums: `enum Message { Move { x: f32 }, Chat(String), Quit }`
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u8; // Use an 8-bit index to tag the variant
                match &variant.fields {
                    Fields::Named(fields) => {
                        // Named variant: `Move { x: f32 }`
                        let field_names = fields.named.iter().map(|f| &f.ident);
                        let serialize_fields = fields.named.iter().map(|f| {
                            let name = &f.ident;
                            // Serialize each field in the variant
                            quote! {
                                writer = #name.serialize(writer)?;
                            }
                        });
                        let field_names_cloned = field_names.clone();
                        quote! {
                            #name::#variant_name { #(#field_names),* } => {
                                // Write the variant index (8 bits)
                                writer = writer.write_bits(#variant_index as u64, 8)?;
                                // Serialize each field
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        // Tuple variant: `Chat(String)`
                        let field_names = (0..fields.unnamed.len()).map(|i| {
                            syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site())
                        }).collect::<Vec<_>>();
                        let serialize_fields = field_names.iter().map(|name| {
                            quote! {
                                writer = #name.serialize(writer)?;
                            }
                        });
                        let field_names_cloned = field_names.clone();
                        quote! {
                            #name::#variant_name(#(#field_names),*) => {
                                // Write the variant index (8 bits)
                                writer = writer.write_bits(#variant_index as u64, 8)?;
                                // Serialize each tuple element
                                #(#serialize_fields)*
                                Ok(writer)
                            }
                        }
                    }
                    Fields::Unit => {
                        // Unit variant: `Quit`
                        quote! {
                            #name::#variant_name => {
                                // Write the variant index (8 bits)
                                writer = writer.write_bits(#variant_index as u64, 8)?;
                                Ok(writer)
                            }
                        }
                    }
                }
            });
            quote! {
                impl #impl_generics ::gbnet::serialize::Serialize for #name #ty_generics #where_clause {
                    fn serialize(&self, writer: ::gbnet::serialize::BitWriter) -> ::std::io::Result<::gbnet::serialize::BitWriter> {
                        match self {
                            #(#variants),*
                        }
                    }
                }
            }
        }
        Data::Union(_) => {
            // Handle unions: `union MyUnion { x: u32, y: f32 }`
            // Unions are unsafe in Rust; we serialize them as raw bytes
            quote! {
                impl #impl_generics ::gbnet::serialize::Serialize for #name #ty_generics #where_clause {
                    fn serialize(&self, writer: ::gbnet::serialize::BitWriter) -> ::std::io::Result<::gbnet::serialize::BitWriter> {
                        // Get the size of the union in bytes
                        let size = ::std::mem::size_of::<Self>();
                        // Convert the union to raw bytes (unsafe but necessary for unions)
                        let bytes = unsafe {
                            ::std::slice::from_raw_parts(self as *const Self as *const u8, size)
                        };
                        // Write the size as a 16-bit prefix
                        let mut writer = writer.write_bits(size as u64, 16)?;
                        // Write each byte (8 bits each)
                        for &byte in bytes {
                            writer = writer.write_bits(byte as u64, 8)?;
                        }
                        Ok(writer)
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derives the `Deserialize` trait for a type, enabling bit-packed deserialization.
/// This macro generates code to deserialize structs, enums, unions, and generics
/// from a `BitReader`, matching the serialization format for networking.
#[proc_macro_derive(Deserialize)]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Add trait bounds to generics (e.g., `T: Deserialize`)
    let generics = add_trait_bounds(input.generics.clone(), quote! { ::gbnet::serialize::Deserialize });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate deserialization code based on the type
    let expanded = match input.data {
        Data::Struct(data) => {
            match data.fields {
                Fields::Named(fields) => {
                    let field_names = fields.named.iter().map(|f| &f.ident);
                    let field_types = fields.named.iter().map(|f| &f.ty);
                    let deserialize_fields = field_names.zip(field_types).map(|(name, ty)| {
                        // Deserialize each field
                        quote! {
                            let (#name, reader) = #ty::deserialize(reader)?;
                        }
                    });
                    let field_names_cloned = field_names.clone();
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Deserialize for #name #ty_generics #where_clause {
                            fn deserialize(reader: ::gbnet::serialize::BitReader) -> ::std::io::Result<(Self, ::gbnet::serialize::BitReader)> {
                                let mut reader = reader;
                                // Deserialize each field in order
                                #(#deserialize_fields)*
                                Ok((
                                    #name {
                                        #(#field_names_cloned,)*
                                    },
                                    reader,
                                ))
                            }
                        }
                    }
                }
                Fields::Unnamed(fields) => {
                    let field_types = fields.unnamed.iter().map(|f| &f.ty);
                    let field_names = (0..fields.unnamed.len()).map(|i| {
                        syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site())
                    }).collect::<Vec<_>>();
                    let deserialize_fields = field_names.iter().zip(field_types).map(|(name, ty)| {
                        quote! {
                            let (#name, reader) = #ty::deserialize(reader)?;
                        }
                    });
                    let field_names_cloned = field_names.clone();
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Deserialize for #name #ty_generics #where_clause {
                            fn deserialize(reader: ::gbnet::serialize::BitReader) -> ::std::io::Result<(Self, ::gbnet::serialize::BitReader)> {
                                let mut reader = reader;
                                // Deserialize each tuple element
                                #(#deserialize_fields)*
                                Ok((
                                    #name(#(#field_names_cloned),*),
                                    reader,
                                ))
                            }
                        }
                    }
                }
                Fields::Unit => {
                    quote! {
                        impl #impl_generics ::gbnet::serialize::Deserialize for #name #ty_generics #where_clause {
                            fn deserialize(reader: ::gbnet::serialize::BitReader) -> ::std::io::Result<(Self, ::gbnet::serialize::BitReader)> {
                                // Unit struct: nothing to deserialize
                                Ok((#name, reader))
                            }
                        }
                    }
                }
            }
        }
        Data::Enum(data) => {
            let variants = data.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;
                let variant_index = i as u8;
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names = fields.named.iter().map(|f| &f.ident);
                        let field_types = fields.named.iter().map(|f| &f.ty);
                        let deserialize_fields = field_names.zip(field_types).map(|(name, ty)| {
                            quote! {
                                let (#name, reader) = #ty::deserialize(reader)?;
                            }
                        });
                        let field_names_cloned = field_names.clone();
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok((#name::#variant_name { #(#field_names_cloned,)* }, reader))
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_types = fields.unnamed.iter().map(|f| &f.ty);
                        let field_names = (0..fields.unnamed.len()).map(|i| {
                            syn::Ident::new(&format!("field_{}", i), proc_macro2::Span::call_site())
                        }).collect::<Vec<_>>();
                        let deserialize_fields = field_names.iter().zip(field_types).map(|(name, ty)| {
                            quote! {
                                let (#name, reader) = #ty::deserialize(reader)?;
                            }
                        });
                        let field_names_cloned = field_names.clone();
                        quote! {
                            #variant_index => {
                                #(#deserialize_fields)*
                                Ok((#name::#variant_name(#(#field_names_cloned),*), reader))
                            }
                        }
                    }
                    Fields::Unit => {
                        quote! {
                            #variant_index => Ok((#name::#variant_name, reader))
                        }
                    }
                }
            });
            quote! {
                impl #impl_generics ::gbnet::serialize::Deserialize for #name #ty_generics #where_clause {
                    fn deserialize(reader: ::gbnet::serialize::BitReader) -> ::std::io::Result<(Self, ::gbnet::serialize::BitReader)> {
                        // Read the 8-bit variant index
                        let (variant_index, reader) = reader.read_bits(8)?;
                        match variant_index as u8 {
                            #(#variants),*
                            _ => Err(::std::io::Error::new(::std::io::ErrorKind::InvalidData, "Unknown variant index")),
                        }
                    }
                }
            }
        }
        Data::Union(_) => {
            quote! {
                impl #impl_generics ::gbnet::serialize::Deserialize for #name #ty_generics #where_clause {
                    fn deserialize(reader: ::gbnet::serialize::BitReader) -> ::std::io::Result<(Self, ::gbnet::serialize::BitReader)> {
                        // Read the 16-bit size prefix
                        let (size, reader) = reader.read_bits(16)?;
                        let mut bytes = vec![0u8; size as usize];
                        // Read the raw bytes
                        let reader = bytes.iter_mut().fold(reader, |r, b| {
                            let (byte, r) = r.read_bits(8)?;
                            *b = byte as u8;
                            Ok(r)
                        })?;
                        // Reconstruct the union (unsafe)
                        let mut value: Self = unsafe { ::std::mem::zeroed() };
                        unsafe {
                            ::std::ptr::copy_nonoverlapping(bytes.as_ptr(), &mut value as *mut _ as *mut u8, size as usize);
                        }
                        Ok((value, reader))
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Adds `Serialize` or `Deserialize` trait bounds to all generic type parameters.
/// This ensures that types like `struct Wrapper<T>` require `T: Serialize`.
fn add_trait_bounds(mut generics: Generics, bound: proc_macro2::TokenStream) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(syn::parse2(bound.clone()).unwrap());
        }
    }
    generics
}