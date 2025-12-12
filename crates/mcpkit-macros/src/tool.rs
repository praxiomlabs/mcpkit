//! Implementation of the `#[tool]` attribute macro.
//!
//! This module handles parsing and code generation for tool methods.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Error, ImplItemFn, Result};

use crate::attrs::ToolAttrs;

/// Expand the `#[tool]` attribute.
///
/// When used standalone (not inside `#[mcp_server]`), this macro
/// preserves the method but adds metadata that `#[mcp_server]` can discover.
pub fn expand_tool(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse the attribute
    let attrs =
        ToolAttrs::parse(attr).map_err(|e| Error::new(proc_macro2::Span::call_site(), e))?;

    // Parse the method
    let method: ImplItemFn = parse2(item)?;

    // Validate the method signature
    validate_tool_method(&method)?;

    // For now, just preserve the method with a marker attribute
    // The actual code generation happens in mcp_server
    let description = &attrs.description;
    let tool_name = attrs.name.unwrap_or_else(|| method.sig.ident.to_string());
    // TODO: Use these attributes in generated code when implementing tool metadata
    let _ = (attrs.destructive, attrs.idempotent, attrs.read_only);

    // Generate a hidden constant that mcp_server can find
    let marker_name = syn::Ident::new(&format!("__MCP_TOOL_{tool_name}"), method.sig.ident.span());

    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #marker_name: () = ();

        #[doc = #description]
        #[allow(dead_code)]
        #method
    })
}

/// Validate that a method has a valid signature for a tool.
fn validate_tool_method(method: &ImplItemFn) -> Result<()> {
    // Must have &self receiver
    if method.sig.receiver().is_none() {
        return Err(Error::new_spanned(
            &method.sig,
            "tool methods must take &self",
        ));
    }

    // Check that receiver is &self (not &mut self or self)
    if let Some(receiver) = method.sig.receiver() {
        if receiver.mutability.is_some() {
            return Err(Error::new_spanned(
                receiver,
                "tool methods should take &self, not &mut self\n\
                 help: use interior mutability (e.g., Mutex, RwLock) if you need to modify state",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_validate_tool_method() {
        // Valid method
        let method: ImplItemFn = parse_quote! {
            async fn add(&self, a: f64, b: f64) -> ToolOutput {
                ToolOutput::text((a + b).to_string())
            }
        };
        assert!(validate_tool_method(&method).is_ok());

        // Method without self - invalid
        let method: ImplItemFn = parse_quote! {
            async fn add(a: f64, b: f64) -> ToolOutput {
                ToolOutput::text((a + b).to_string())
            }
        };
        assert!(validate_tool_method(&method).is_err());
    }
}
