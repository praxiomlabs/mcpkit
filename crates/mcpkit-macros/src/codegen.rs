//! Code generation utilities for MCP macros.
//!
//! This module provides helpers for generating code in the procedural macros.

#![allow(dead_code)]

use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, Pat, PatIdent, PatType, ReturnType, Type};

use crate::attrs::ParamAttrs;

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
    /// Default value, from `#[mcp(default = ...)]`.
    pub default: Option<syn::Lit>,
    /// Minimum value, from `#[mcp(min = ...)]`.
    pub min: Option<i64>,
    /// Maximum value, from `#[mcp(max = ...)]`.
    pub max: Option<i64>,
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

            // `#[mcp(default = ..., min = ..., max = ...)]` -> JSON Schema
            // `default` / `minimum` / `maximum`.
            let default_insert = param.default.as_ref().map_or_else(
                || quote!(),
                |lit| quote!(obj.insert("default".to_string(), ::serde_json::json!(#lit));),
            );
            let min_insert = param.min.map_or_else(
                || quote!(),
                |m| quote!(obj.insert("minimum".to_string(), ::serde_json::json!(#m));),
            );
            let max_insert = param.max.map_or_else(
                || quote!(),
                |m| quote!(obj.insert("maximum".to_string(), ::serde_json::json!(#m));),
            );

            properties.push(quote! {
                (#name.to_string(), {
                    let mut prop = #type_schema;
                    if let ::serde_json::Value::Object(ref mut obj) = prop {
                        if #description != ::serde_json::Value::Null {
                            obj.insert("description".to_string(), #description);
                        }
                        #default_insert
                        #min_insert
                        #max_insert
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
                    // Insert in declaration order rather than collecting through a
                    // HashMap, whose iteration order is randomized per run and would
                    // make tools/list emit non-deterministically ordered schemas.
                    let mut properties = ::serde_json::Map::new();
                    for (name, value) in vec![#(#properties),*] {
                        properties.insert(name, value);
                    }
                    obj.insert("properties".to_string(), ::serde_json::Value::Object(properties));
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
        // Match on the *last* path segment ident so qualified paths such as
        // `std::string::String` or `core::option::Option<T>` resolve correctly,
        // rather than stringifying the whole path (which only matched bare names).
        let Some(segment) = path.path.segments.last() else {
            return quote!(::serde_json::json!({}));
        };

        match segment.ident.to_string().as_str() {
            "String" | "str" => quote!(::serde_json::json!({"type": "string"})),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => {
                quote!(::serde_json::json!({"type": "integer"}))
            }
            "f32" | "f64" => quote!(::serde_json::json!({"type": "number"})),
            "bool" => quote!(::serde_json::json!({"type": "boolean"})),
            "Option" => {
                // Optionality is conveyed by omitting the parameter from the
                // schema's `required` array (see `generate_input_schema`); the
                // value schema is just the inner type's schema.
                first_type_arg(segment)
                    .map_or_else(|| quote!(::serde_json::json!({})), type_to_json_schema)
            }
            "Vec" => first_type_arg(segment).map_or_else(
                || quote!(::serde_json::json!({"type": "array"})),
                |inner| {
                    let inner_schema = type_to_json_schema(inner);
                    quote!(::serde_json::json!({"type": "array", "items": #inner_schema}))
                },
            ),
            "HashMap" | "BTreeMap" => {
                quote!(::serde_json::json!({"type": "object", "additionalProperties": true}))
            }
            "Value" => quote!(::serde_json::json!({})),
            _ => {
                // For custom struct types, call their tool_input_schema() method.
                // This requires the type to derive ToolInput; otherwise the user
                // gets a compile error pointing at the type.
                quote!(#path::tool_input_schema())
            }
        }
    } else {
        quote!(::serde_json::json!({}))
    }
}

/// Return the first generic type argument of a path segment (e.g. `T` in
/// `Option<T>` or `Vec<T>`), if present.
fn first_type_arg(segment: &syn::PathSegment) -> Option<&Type> {
    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
            return Some(inner);
        }
    }
    None
}

/// Extract parameter information from a function argument.
///
/// Parses (and strips) the `#[mcp(default = ..., min = ..., max = ...)]` helper
/// attribute so it does not leak into the re-emitted impl block. Returns an
/// error for a malformed `#[mcp(...)]` attribute instead of silently ignoring it.
pub fn extract_param(arg: &mut FnArg) -> syn::Result<Option<ToolParam>> {
    match arg {
        FnArg::Typed(PatType { pat, ty, attrs, .. }) => {
            // Get the parameter name
            let name = match pat.as_ref() {
                Pat::Ident(PatIdent { ident, .. }) => ident.clone(),
                _ => return Ok(None),
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

            // Parse the `#[mcp(...)]` helper attribute, if present.
            let mut param_attrs = ParamAttrs::default();
            for attr in attrs.iter() {
                if attr.path().is_ident("mcp") {
                    param_attrs = ParamAttrs::from_meta(&attr.meta)
                        .map_err(|e| syn::Error::new_spanned(attr, e.to_string()))?;
                }
            }
            // Strip the attributes the macro consumes (`#[mcp(...)]` and the doc
            // comments read above) so they aren't re-emitted onto the parameter,
            // where the compiler rejects all but a few built-in attributes.
            attrs.retain(|attr| !attr.path().is_ident("mcp") && !attr.path().is_ident("doc"));

            // Check if optional
            let is_optional = is_option_type(ty);

            Ok(Some(ToolParam {
                name,
                ty: (**ty).clone(),
                doc,
                is_optional,
                default: param_attrs.default,
                min: param_attrs.min,
                max: param_attrs.max,
            }))
        }
        FnArg::Receiver(_) => Ok(None),
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
