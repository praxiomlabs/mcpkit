//! Implementation of derive macros for MCP types.
//!
//! This module provides derive macros for automatically generating
//! JSON Schema and other metadata for tool input types.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Data, DeriveInput, Error, Fields, GenericArgument, PathArguments, Result, Type};

/// Expand the `#[derive(ToolInput)]` macro.
///
/// This generates a `tool_input_schema` method that returns JSON Schema
/// for the struct, suitable for use as tool input.
pub fn expand_tool_input(input: TokenStream) -> Result<TokenStream> {
    let input: DeriveInput = parse2(input)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Only works on structs
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(_) => {
                return Err(Error::new_spanned(
                    name,
                    "ToolInput can only be derived for structs with named fields",
                ))
            }
            Fields::Unit => {
                return Err(Error::new_spanned(
                    name,
                    "ToolInput cannot be derived for unit structs",
                ))
            }
        },
        Data::Enum(_) => {
            return Err(Error::new_spanned(
                name,
                "ToolInput can only be derived for structs, not enums",
            ))
        }
        Data::Union(_) => {
            return Err(Error::new_spanned(
                name,
                "ToolInput can only be derived for structs, not unions",
            ))
        }
    };

    // Generate property schema for each field
    let mut property_schemas = Vec::new();
    let mut required_fields = Vec::new();

    for field in fields {
        // Named fields always have idents (tuple struct fields filtered above)
        let field_name = field
            .ident
            .as_ref()
            .expect("expected named field but found tuple field - this should not happen");
        let field_name_str = field_name.to_string();
        let field_ty = &field.ty;

        // Extract doc comment as description
        let description = extract_doc_comment(&field.attrs);

        // Check if field is optional
        let is_optional = is_option_type(field_ty);

        // Generate schema for the field type
        let type_schema = type_to_json_schema(field_ty);

        // Add description if available
        let schema_with_desc = if let Some(desc) = &description {
            quote! {
                {
                    let mut schema = #type_schema;
                    if let serde_json::Value::Object(ref mut obj) = schema {
                        obj.insert("description".to_string(), serde_json::Value::String(#desc.to_string()));
                    }
                    schema
                }
            }
        } else {
            type_schema
        };

        property_schemas.push(quote! {
            properties.insert(#field_name_str.to_string(), #schema_with_desc);
        });

        if !is_optional {
            required_fields.push(field_name_str);
        }
    }

    let required_array = if required_fields.is_empty() {
        quote!(serde_json::Value::Null)
    } else {
        quote! {
            serde_json::Value::Array(
                vec![#(serde_json::Value::String(#required_fields.to_string())),*]
            )
        }
    };

    let struct_name_str = name.to_string();

    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Generate JSON Schema for this type.
            pub fn tool_input_schema() -> serde_json::Value {
                let mut properties = serde_json::Map::new();
                #(#property_schemas)*

                let mut schema = serde_json::json!({
                    "type": "object",
                    "title": #struct_name_str,
                });

                if let serde_json::Value::Object(ref mut obj) = schema {
                    obj.insert("properties".to_string(), serde_json::Value::Object(properties));

                    let required = #required_array;
                    if required != serde_json::Value::Null {
                        obj.insert("required".to_string(), required);
                    }
                }

                schema
            }
        }
    })
}

/// Extract doc comments from attributes.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &attr.meta {
                    if let syn::Expr::Lit(lit) = &nv.value {
                        if let syn::Lit::Str(s) = &lit.lit {
                            return Some(s.value().trim().to_string());
                        }
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// Check if a type is Option<T>.
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Get the inner type of Option<T>.
fn get_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.last() {
            if segment.ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return Some(inner);
                    }
                }
            }
        }
    }
    None
}

/// Convert a Rust type to a JSON Schema representation.
fn type_to_json_schema(ty: &Type) -> TokenStream {
    if let Type::Path(path) = ty {
        let path_str = quote!(#path).to_string().replace(' ', "");

        match path_str.as_str() {
            "String" | "&str" | "str" => quote!(serde_json::json!({"type": "string"})),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                quote!(serde_json::json!({"type": "integer"}))
            }
            "f32" | "f64" => quote!(serde_json::json!({"type": "number"})),
            "bool" => quote!(serde_json::json!({"type": "boolean"})),
            _ if path_str.starts_with("Option<") => {
                // Extract inner type
                if let Some(inner) = get_option_inner_type(ty) {
                    let inner_schema = type_to_json_schema(inner);
                    return inner_schema;
                }
                quote!(serde_json::json!({}))
            }
            _ if path_str.starts_with("Vec<") => {
                // Handle Vec<T>
                if let Some(segment) = path.path.segments.last() {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let inner_schema = type_to_json_schema(inner);
                            return quote! {
                                serde_json::json!({
                                    "type": "array",
                                    "items": #inner_schema
                                })
                            };
                        }
                    }
                }
                quote!(serde_json::json!({"type": "array"}))
            }
            _ if path_str.starts_with("HashMap<") || path_str.starts_with("BTreeMap<") => {
                quote!(serde_json::json!({
                    "type": "object",
                    "additionalProperties": true
                }))
            }
            _ => {
                // Default to object for unknown types
                quote!(serde_json::json!({"type": "object"}))
            }
        }
    } else {
        quote!(serde_json::json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_is_option_type() {
        let ty: Type = parse_quote!(Option<String>);
        assert!(is_option_type(&ty));

        let ty: Type = parse_quote!(String);
        assert!(!is_option_type(&ty));
    }

    #[test]
    fn test_extract_doc_comment() {
        let attrs: Vec<syn::Attribute> = vec![parse_quote!(#[doc = " This is a test "])];
        assert_eq!(
            extract_doc_comment(&attrs),
            Some("This is a test".to_string())
        );
    }
}
