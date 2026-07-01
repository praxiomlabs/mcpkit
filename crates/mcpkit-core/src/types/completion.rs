//! Completion types for MCP argument completion.
//!
//! This module provides types for the completion capability
//! which enables auto-completion of arguments.

use super::meta::Meta;
use serde::{Deserialize, Serialize};

/// Reference to a prompt or resource for completion context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompletionRef {
    /// Reference to a prompt.
    #[serde(rename = "ref/prompt")]
    Prompt {
        /// The prompt name.
        name: String,
    },
    /// Reference to a resource.
    #[serde(rename = "ref/resource")]
    Resource {
        /// The resource URI.
        uri: String,
    },
}

impl CompletionRef {
    /// Create a prompt reference.
    pub fn prompt(name: impl Into<String>) -> Self {
        Self::Prompt { name: name.into() }
    }

    /// Create a resource reference.
    pub fn resource(uri: impl Into<String>) -> Self {
        Self::Resource { uri: uri.into() }
    }

    /// Get the reference type as a string.
    #[must_use]
    pub const fn ref_type(&self) -> &'static str {
        match self {
            Self::Prompt { .. } => "ref/prompt",
            Self::Resource { .. } => "ref/resource",
        }
    }

    /// Get the reference value (name or URI).
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::Prompt { name } => name,
            Self::Resource { uri } => uri,
        }
    }
}

/// Argument for completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionArgument {
    /// Argument name.
    pub name: String,
    /// Current value being typed.
    pub value: String,
}

/// Request for argument completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRequest {
    /// Reference to the prompt or resource.
    #[serde(rename = "ref")]
    pub ref_: CompletionRef,
    /// Argument to complete.
    pub argument: CompletionArgument,
}

/// Completion result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// Suggested values.
    pub values: Vec<String>,
    /// Total number of available completions (if known).
    ///
    /// Per the MCP spec this is a plain count; whether more are available beyond
    /// what was returned is conveyed separately by [`has_more`](Self::has_more).
    pub total: Option<usize>,
    /// Whether there are more completions available.
    #[serde(rename = "hasMore")]
    pub has_more: Option<bool>,
}

/// Result of a completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteResult {
    /// The completion data.
    pub completion: Completion,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_total_is_a_plain_number() {
        // #17: total serializes as a bare integer and round-trips (the old
        // CompletionTotal enum made the "approximate" variant unreachable).
        let completion = Completion {
            values: vec!["a".to_string()],
            total: Some(42),
            has_more: Some(true),
        };
        let json = serde_json::to_value(&completion).unwrap();
        assert_eq!(json["total"], serde_json::json!(42));
        assert_eq!(json["hasMore"], serde_json::json!(true));

        let parsed: Completion = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.total, Some(42));
    }

    #[test]
    fn test_completion_ref() {
        let prompt_ref = CompletionRef::prompt("code-review");
        assert_eq!(prompt_ref.ref_type(), "ref/prompt");
        assert_eq!(prompt_ref.value(), "code-review");

        let resource_ref = CompletionRef::resource("file:///test.txt");
        assert_eq!(resource_ref.ref_type(), "ref/resource");
        assert_eq!(resource_ref.value(), "file:///test.txt");
    }

    #[test]
    fn test_complete_request_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let request = CompleteRequest {
            ref_: CompletionRef::prompt("test"),
            argument: CompletionArgument {
                name: "arg1".to_string(),
                value: "val".to_string(),
            },
        };

        let json = serde_json::to_string(&request)?;
        assert!(json.contains("ref/prompt"));

        let parsed: CompleteRequest = serde_json::from_str(&json)?;
        assert_eq!(parsed.argument.name, "arg1");
        Ok(())
    }
}
