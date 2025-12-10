//! Attribute parsing for MCP macros.
//!
//! This module uses darling to parse macro attributes into structured types.

#![allow(dead_code)]

use darling::{ast::NestedMeta, FromMeta};
use proc_macro2::Span;

/// Attributes for the `#[mcp_server]` macro.
#[derive(Debug, FromMeta)]
pub struct ServerAttrs {
    /// Server name (required).
    pub name: String,

    /// Server version (required).
    pub version: String,

    /// Optional usage instructions.
    #[darling(default)]
    pub instructions: Option<String>,

    /// Debug mode - print expanded code.
    #[darling(default)]
    pub debug_expand: bool,
}

impl ServerAttrs {
    /// Parse server attributes from attribute tokens.
    pub fn parse(attr: proc_macro2::TokenStream) -> Result<Self, darling::Error> {
        let attr_args = NestedMeta::parse_meta_list(attr)?;
        Self::from_list(&attr_args)
    }
}

/// Attributes for the `#[tool]` macro.
#[derive(Debug, FromMeta)]
pub struct ToolAttrs {
    /// Tool description (required).
    pub description: String,

    /// Override the tool name (defaults to method name).
    #[darling(default)]
    pub name: Option<String>,

    /// Hint that the tool may cause destructive changes.
    #[darling(default)]
    pub destructive: bool,

    /// Hint that calling the tool multiple times has same effect.
    #[darling(default)]
    pub idempotent: bool,

    /// Hint that the tool only reads data.
    #[darling(default)]
    pub read_only: bool,
}

impl ToolAttrs {
    /// Parse tool attributes from attribute tokens.
    pub fn parse(attr: proc_macro2::TokenStream) -> Result<Self, darling::Error> {
        let attr_args = NestedMeta::parse_meta_list(attr)?;
        Self::from_list(&attr_args)
    }
}

/// Attributes for the `#[resource]` macro.
#[derive(Debug, FromMeta)]
pub struct ResourceAttrs {
    /// URI pattern for the resource.
    pub uri_pattern: String,

    /// Human-readable name.
    #[darling(default)]
    pub name: Option<String>,

    /// Resource description.
    #[darling(default)]
    pub description: Option<String>,

    /// MIME type of the resource content.
    #[darling(default)]
    pub mime_type: Option<String>,
}

impl ResourceAttrs {
    /// Parse resource attributes from attribute tokens.
    pub fn parse(attr: proc_macro2::TokenStream) -> Result<Self, darling::Error> {
        let attr_args = NestedMeta::parse_meta_list(attr)?;
        Self::from_list(&attr_args)
    }
}

/// Attributes for the `#[prompt]` macro.
#[derive(Debug, FromMeta)]
pub struct PromptAttrs {
    /// Prompt description (required).
    pub description: String,

    /// Override the prompt name (defaults to method name).
    #[darling(default)]
    pub name: Option<String>,
}

impl PromptAttrs {
    /// Parse prompt attributes from attribute tokens.
    pub fn parse(attr: proc_macro2::TokenStream) -> Result<Self, darling::Error> {
        let attr_args = NestedMeta::parse_meta_list(attr)?;
        Self::from_list(&attr_args)
    }
}

/// Attributes for the `#[mcp(...)]` helper attribute on parameters.
#[derive(Debug, Default, FromMeta)]
pub struct ParamAttrs {
    /// Default value for the parameter.
    #[darling(default)]
    pub default: Option<syn::Lit>,

    /// Minimum value (for numeric types).
    #[darling(default)]
    pub min: Option<i64>,

    /// Maximum value (for numeric types).
    #[darling(default)]
    pub max: Option<i64>,
}

/// Create a span for error messages.
pub fn call_site() -> Span {
    Span::call_site()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_attrs_parse() {
        let tokens = quote::quote!(name = "test-server", version = "1.0.0");
        let attrs = ServerAttrs::parse(tokens).unwrap();
        assert_eq!(attrs.name, "test-server");
        assert_eq!(attrs.version, "1.0.0");
        assert!(attrs.instructions.is_none());
        assert!(!attrs.debug_expand);
    }

    #[test]
    fn test_tool_attrs_parse() {
        let tokens = quote::quote!(description = "Test tool", destructive = true);
        let attrs = ToolAttrs::parse(tokens).unwrap();
        assert_eq!(attrs.description, "Test tool");
        assert!(attrs.destructive);
        assert!(!attrs.idempotent);
    }
}
