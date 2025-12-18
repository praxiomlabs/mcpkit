//! Rich compile-time error handling for MCP macros.
//!
//! This module provides helpful error messages with suggestions
//! for common mistakes.

#![allow(dead_code)]

use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;

/// Known attribute names for `#[mcp_server]`.
const SERVER_ATTRS: &[&str] = &["name", "version", "instructions", "debug_expand"];

/// Known attribute names for `#[tool]`.
const TOOL_ATTRS: &[&str] = &[
    "description",
    "name",
    "destructive",
    "idempotent",
    "read_only",
];

/// Known attribute names for `#[resource]`.
const RESOURCE_ATTRS: &[&str] = &["uri_pattern", "name", "description", "mime_type"];

/// Known attribute names for `#[prompt]`.
const PROMPT_ATTRS: &[&str] = &["description", "name"];

/// Create an error for an unknown attribute with suggestions.
pub fn unknown_attr_error(attr_name: &str, context: AttrContext, span: Span) -> TokenStream {
    let known = match context {
        AttrContext::Server => SERVER_ATTRS,
        AttrContext::Tool => TOOL_ATTRS,
        AttrContext::Resource => RESOURCE_ATTRS,
        AttrContext::Prompt => PROMPT_ATTRS,
    };

    let suggestion = find_similar(attr_name, known);
    let known_list = known.join(", ");

    let message = if let Some(similar) = suggestion {
        format!(
            "unknown attribute `{attr_name}`\n\n\
             help: did you mean `{similar}`?\n\
             note: valid attributes are: {known_list}"
        )
    } else {
        format!(
            "unknown attribute `{attr_name}`\n\n\
             note: valid attributes are: {known_list}"
        )
    };

    quote_spanned!(span => compile_error!(#message);)
}

/// Create an error for a missing required attribute.
pub fn missing_attr_error(attr_name: &str, context: AttrContext, span: Span) -> TokenStream {
    let context_name = match context {
        AttrContext::Server => "mcp_server",
        AttrContext::Tool => "tool",
        AttrContext::Resource => "resource",
        AttrContext::Prompt => "prompt",
    };

    let message = format!(
        "missing required attribute `{attr_name}` for #[{context_name}]\n\n\
         help: add `{attr_name} = \"...\"` to the attribute"
    );

    quote_spanned!(span => compile_error!(#message);)
}

/// Create an error for an invalid attribute value.
pub fn invalid_value_error(attr_name: &str, expected: &str, got: &str, span: Span) -> TokenStream {
    let message = format!("invalid value for `{attr_name}`: expected {expected}, got `{got}`");

    quote_spanned!(span => compile_error!(#message);)
}

/// Create an error for using tool outside `mcp_server`.
pub fn tool_outside_server_error(span: Span) -> TokenStream {
    let message = "\
        #[tool] must be used inside an #[mcp_server] impl block\n\n\
        help: wrap your impl block with #[mcp_server(name = \"...\", version = \"...\")]";

    quote_spanned!(span => compile_error!(#message);)
}

/// Create an error for invalid method signature.
pub fn invalid_signature_error(issue: &str, span: Span) -> TokenStream {
    let message = format!("invalid method signature: {issue}");
    quote_spanned!(span => compile_error!(#message);)
}

/// The context in which an attribute is being used.
#[derive(Debug, Clone, Copy)]
pub enum AttrContext {
    /// `#[mcp_server]` attribute
    Server,
    /// `#[tool]` attribute
    Tool,
    /// `#[resource]` attribute
    Resource,
    /// `#[prompt]` attribute
    Prompt,
}

/// Find a similar string in a list (for typo suggestions).
fn find_similar<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase();

    // First try prefix match
    for candidate in candidates {
        if candidate.starts_with(&input_lower) || input_lower.starts_with(candidate) {
            return Some(candidate);
        }
    }

    // Then try edit distance
    candidates
        .iter()
        .filter_map(|candidate| {
            let dist = levenshtein(&input_lower, candidate);
            if dist <= 2 {
                Some((*candidate, dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, dist)| *dist)
        .map(|(candidate, _)| candidate)
}

/// Compute Levenshtein distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate() {
        *cell = j;
    }

    for (i, a_char) in a.chars().enumerate() {
        for (j, b_char) in b.chars().enumerate() {
            let cost = usize::from(a_char != b_char);
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar() {
        assert_eq!(find_similar("descripion", TOOL_ATTRS), Some("description"));
        assert_eq!(find_similar("nam", TOOL_ATTRS), Some("name"));
        assert_eq!(find_similar("xyz123", TOOL_ATTRS), None);
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("abc", "abcd"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }
}
