//! Implementation of the `#[resource]` attribute macro.
//!
//! This module handles parsing and code generation for resource methods.
//!
//! The main entry point is [`expand_resource`], which transforms methods annotated
//! with `#[resource]` into methods with metadata markers that `#[mcp_server]` can discover.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Error, ImplItemFn, Result};

use crate::attrs::ResourceAttrs;

/// Expand the `#[resource]` attribute.
///
/// When used standalone (not inside `#[mcp_server]`), this macro
/// preserves the method but adds metadata that `#[mcp_server]` can discover.
pub fn expand_resource(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse the attribute
    let attrs =
        ResourceAttrs::parse(attr).map_err(|e| Error::new(proc_macro2::Span::call_site(), e))?;

    // Parse the method
    let method: ImplItemFn = parse2(item)?;

    // Validate the method signature
    validate_resource_method(&method)?;

    // Extract info
    let uri_pattern = &attrs.uri_pattern;
    let resource_name = attrs.name.unwrap_or_else(|| method.sig.ident.to_string());
    let description = attrs.description.unwrap_or_default();
    let mime_type = attrs.mime_type.unwrap_or_else(|| "text/plain".to_string());

    // Generate a hidden constant that mcp_server can find
    let marker_name = syn::Ident::new(
        &format!("__MCP_RESOURCE_{}", method.sig.ident),
        method.sig.ident.span(),
    );

    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #marker_name: (&str, &str, &str, &str) = (#uri_pattern, #resource_name, #description, #mime_type);

        #[doc = #description]
        #[allow(dead_code)]
        #method
    })
}

/// Validate that a method has a valid signature for a resource.
fn validate_resource_method(method: &ImplItemFn) -> Result<()> {
    // Must have &self receiver
    if method.sig.receiver().is_none() {
        return Err(Error::new_spanned(
            &method.sig,
            "resource methods must take &self",
        ));
    }

    // Check that receiver is &self (not &mut self or self)
    if let Some(receiver) = method.sig.receiver() {
        if receiver.mutability.is_some() {
            return Err(Error::new_spanned(
                receiver,
                "resource methods should take &self, not &mut self\n\
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
    fn test_validate_resource_method() {
        // Valid method
        let method: ImplItemFn = parse_quote! {
            async fn get_config(&self, key: String) -> ResourceContents {
                ResourceContents::text("value")
            }
        };
        assert!(validate_resource_method(&method).is_ok());

        // Method without self - invalid
        let method: ImplItemFn = parse_quote! {
            async fn get_config(key: String) -> ResourceContents {
                ResourceContents::text("value")
            }
        };
        assert!(validate_resource_method(&method).is_err());
    }
}
