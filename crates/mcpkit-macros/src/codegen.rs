//! Code generation utilities for MCP macros.
//!
//! This module provides helpers for generating code in the procedural macros.

#![allow(dead_code)]

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, Pat, PatIdent, PatType, ReturnType, Type};

/// Information about a tool method extracted from the AST.
#[derive(Debug)]
pub struct ToolMethod {
    /// The method name
    pub name: Ident,
    /// The tool name (may be overridden by attribute)
    pub tool_name: String,
    /// The description
    pub description: String,
    /// Whether the tool is destructive
    pub destructive: bool,
    /// Whether the tool is idempotent
    pub idempotent: bool,
    /// Whether the tool is read-only
    pub read_only: bool,
    /// The parameters (excluding &self)
    pub params: Vec<ToolParam>,
    /// Whether the method is async
    pub is_async: bool,
    /// Whether the return type is Result
    pub returns_result: bool,
}

/// Information about a tool parameter.
#[derive(Debug)]
pub struct ToolParam {
    /// The parameter name
    pub name: Ident,
    /// The parameter type
    pub ty: Type,
    /// Documentation comment (becomes JSON Schema description)
    pub doc: Option<String>,
    /// Whether the parameter is optional (Option<T>)
    pub is_optional: bool,
    /// Default value if specified
    pub default: Option<syn::Lit>,
}

impl ToolMethod {
    /// Generate the JSON Schema for this tool's input.
    pub fn generate_input_schema(&self) -> TokenStream {
        let mut properties = Vec::new();
        let mut required = Vec::new();

        for param in &self.params {
            let name = param.name.to_string();
            let ty = &param.ty;

            let type_schema = type_to_json_schema(ty);

            let description = param.doc.as_ref().map_or_else(
                || quote!(::serde_json::Value::Null),
                |d| quote!(::serde_json::Value::String(#d.to_string())),
            );

            properties.push(quote! {
                (#name.to_string(), {
                    let mut prop = #type_schema;
                    if let ::serde_json::Value::Object(ref mut obj) = prop {
                        if #description != ::serde_json::Value::Null {
                            obj.insert("description".to_string(), #description);
                        }
                    }
                    prop
                })
            });

            if !param.is_optional {
                required.push(name);
            }
        }

        quote! {
            {
                let mut schema = ::serde_json::json!({
                    "type": "object",
                    "properties": {},
                });
                if let ::serde_json::Value::Object(ref mut obj) = schema {
                    let properties: std::collections::HashMap<String, ::serde_json::Value> =
                        vec![#(#properties),*].into_iter().collect();
                    obj.insert("properties".to_string(), ::serde_json::Value::Object(
                        properties.into_iter().collect()
                    ));
                    let required: Vec<String> = vec![#(#required.to_string()),*];
                    if !required.is_empty() {
                        obj.insert("required".to_string(), ::serde_json::Value::Array(
                            required.into_iter().map(::serde_json::Value::String).collect()
                        ));
                    }
                }
                schema
            }
        }
    }

    /// Generate the tool call dispatch code.
    pub fn generate_call_dispatch(&self) -> TokenStream {
        let method_name = &self.name;
        let tool_name = &self.tool_name;

        // Generate parameter extraction
        let param_extractions: Vec<_> = self
            .params
            .iter()
            .map(|param| {
                let name = &param.name;
                let name_str = name.to_string();
                let ty = &param.ty;

                if param.is_optional {
                    quote! {
                        let #name: #ty = args.get(#name_str)
                            .and_then(|v| ::serde_json::from_value(v.clone()).ok());
                    }
                } else {
                    // Use a different variable name for the Value to avoid type conflict
                    let value_var = quote::format_ident!("__{}_value", name);
                    quote! {
                        let #value_var = args.get(#name_str)
                            .ok_or_else(|| ::mcpkit::error::McpError::invalid_params(
                                #tool_name,
                                format!("missing required parameter: {}", #name_str),
                            ))?
                            .clone();
                        let #name: #ty = ::serde_json::from_value(#value_var)
                            .map_err(|e| ::mcpkit::error::McpError::invalid_params(
                                #tool_name,
                                format!("invalid parameter '{}': {}", #name_str, e),
                            ))?;
                    }
                }
            })
            .collect();

        let param_names: Vec<_> = self.params.iter().map(|p| &p.name).collect();

        let call = if self.is_async {
            quote!(self.#method_name(#(#param_names),*).await)
        } else {
            quote!(self.#method_name(#(#param_names),*))
        };

        let call_with_conversion = if self.returns_result {
            quote!(#call)
        } else {
            quote!(Ok(#call))
        };

        quote! {
            #tool_name => {
                #(#param_extractions)*
                #call_with_conversion
            }
        }
    }
}

/// Convert a Rust type to a JSON Schema representation.
///
/// For primitive types (String, integers, floats, bool), this returns a static schema.
/// For container types (Option, Vec, `HashMap`), it recurses into the inner type.
/// For custom struct types, it attempts to call `Type::tool_input_schema()`, which
/// requires the type to derive `ToolInput`. If the type doesn't have this method,
/// compilation will fail with a clear error message.
fn type_to_json_schema(ty: &Type) -> TokenStream {
    if let Type::Path(path) = ty {
        let path_str = quote!(#path).to_string().replace(' ', "");

        // Handle common types
        match path_str.as_str() {
            "String" | "&str" | "str" => quote!(::serde_json::json!({"type": "string"})),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                quote!(::serde_json::json!({"type": "integer"}))
            }
            "f32" | "f64" => quote!(::serde_json::json!({"type": "number"})),
            "bool" => quote!(::serde_json::json!({"type": "boolean"})),
            _ if path_str.starts_with("Option<") => {
                // Extract inner type and make it nullable
                if let Some(segment) = path.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            let inner_schema = type_to_json_schema(inner);
                            return quote! {
                                {
                                    let mut schema = #inner_schema;
                                    // Option types are nullable
                                    schema
                                }
                            };
                        }
                    }
                }
                quote!(::serde_json::json!({}))
            }
            _ if path_str.starts_with("Vec<") => {
                // Handle Vec<T>
                if let Some(segment) = path.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            let inner_schema = type_to_json_schema(inner);
                            return quote! {
                                ::serde_json::json!({
                                    "type": "array",
                                    "items": #inner_schema
                                })
                            };
                        }
                    }
                }
                quote!(::serde_json::json!({"type": "array"}))
            }
            _ if path_str.starts_with("HashMap<") || path_str.starts_with("BTreeMap<") => {
                quote!(::serde_json::json!({
                    "type": "object",
                    "additionalProperties": true
                }))
            }
            "serde_json::Value" | "Value" => {
                // JSON Value can be any type
                quote!(::serde_json::json!({}))
            }
            _ => {
                // For custom struct types, call their tool_input_schema() method.
                // This requires the type to derive ToolInput.
                // If it doesn't, the user gets a compile error with a helpful message.
                quote!(#path::tool_input_schema())
            }
        }
    } else {
        quote!(::serde_json::json!({}))
    }
}

/// Extract parameter information from a function argument.
pub fn extract_param(arg: &FnArg) -> Option<ToolParam> {
    match arg {
        FnArg::Typed(PatType { pat, ty, attrs, .. }) => {
            // Get the parameter name
            let name = match pat.as_ref() {
                Pat::Ident(PatIdent { ident, .. }) => ident.clone(),
                _ => return None,
            };

            // Extract doc comment
            let doc = attrs
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
                .collect::<Vec<_>>()
                .join(" ");

            let doc = if doc.is_empty() { None } else { Some(doc) };

            // Check if optional
            let is_optional = is_option_type(ty);

            Some(ToolParam {
                name,
                ty: (**ty).clone(),
                doc,
                is_optional,
                default: None,
            })
        }
        FnArg::Receiver(_) => None,
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

/// Check if a return type is Result.
pub fn is_result_type(ret: &ReturnType) -> bool {
    match ret {
        ReturnType::Type(_, ty) => {
            if let Type::Path(path) = ty.as_ref() {
                if let Some(segment) = path.path.segments.last() {
                    return segment.ident == "Result";
                }
            }
            false
        }
        ReturnType::Default => false,
    }
}

/// Generate a unique identifier.
pub fn gen_ident(base: &str, suffix: usize) -> Ident {
    format_ident!("{}_{}", base, suffix)
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
    fn test_is_result_type() {
        let ret: ReturnType = parse_quote!(-> Result<ToolOutput, McpError>);
        assert!(is_result_type(&ret));

        let ret: ReturnType = parse_quote!(-> ToolOutput);
        assert!(!is_result_type(&ret));
    }
}
