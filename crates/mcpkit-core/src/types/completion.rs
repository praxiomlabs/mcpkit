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

/// Additional context for resolving a completion (spec `context`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionContext {
    /// Previously-resolved variables in a URI template or prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<std::collections::BTreeMap<String, String>>,
}

/// Request for argument completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRequest {
    /// Reference to the prompt or resource.
    #[serde(rename = "ref")]
    pub ref_: CompletionRef,
    /// Argument to complete.
    pub argument: CompletionArgument,
    /// Additional, previously-resolved context (e.g. earlier template variables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<CompletionContext>,
}

/// The maximum number of completion values allowed on the wire per the spec.
pub const MAX_COMPLETION_VALUES: usize = 100;

/// Completion result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// Suggested values. The spec caps this at [`MAX_COMPLETION_VALUES`].
    pub values: Vec<String>,
    /// Total number of available completions (if known). May exceed the number
    /// of `values` actually returned.
    ///
    /// Per the MCP spec this is a plain count; whether more are available beyond
    /// what was returned is conveyed separately by [`has_more`](Self::has_more).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Whether there are more completions available beyond `values`.
    #[serde(rename = "hasMore", default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl Completion {
    /// Create a completion from a list of values, reporting `total` as the
    /// number of values and no further pages.
    #[must_use]
    pub fn new(values: Vec<String>) -> Self {
        let total = values.len();
        Self {
            values,
            total: Some(total),
            has_more: Some(false),
        }
    }

    /// Enforce the spec's [`MAX_COMPLETION_VALUES`] cap: truncate `values` to the
    /// limit and, if truncation occurred, force `has_more` to `true` so the wire
    /// stays valid regardless of what a handler returned.
    #[must_use]
    pub fn capped(mut self) -> Self {
        if self.values.len() > MAX_COMPLETION_VALUES {
            self.values.truncate(MAX_COMPLETION_VALUES);
            self.has_more = Some(true);
        }
        self
    }
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

impl From<Completion> for CompleteResult {
    fn from(completion: Completion) -> Self {
        Self {
            completion,
            meta: None,
        }
    }
}

impl CompleteResult {
    /// Attach result-level `_meta`.
    #[must_use]
    pub fn with_meta(mut self, meta: Meta) -> Self {
        self.meta = Some(meta);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_truncates_to_100_and_forces_has_more() {
        let over = Completion {
            values: (0..150).map(|i| i.to_string()).collect(),
            total: Some(150),
            has_more: Some(false),
        }
        .capped();
        assert_eq!(over.values.len(), MAX_COMPLETION_VALUES);
        assert_eq!(over.has_more, Some(true));
        assert_eq!(over.total, Some(150)); // handler-reported total is preserved

        // At or under the cap is left untouched.
        let under = Completion::new(vec!["a".to_string()]).capped();
        assert_eq!(under.values.len(), 1);
        assert_eq!(under.has_more, Some(false));
    }

    #[test]
    fn context_round_trips_and_omits_when_empty() -> Result<(), Box<dyn std::error::Error>> {
        let req: CompleteRequest = serde_json::from_value(serde_json::json!({
            "ref": {"type": "ref/prompt", "name": "p"},
            "argument": {"name": "a", "value": "v"},
            "context": {"arguments": {"owner": "acme"}}
        }))?;
        let args = req
            .context
            .as_ref()
            .and_then(|c| c.arguments.as_ref())
            .ok_or("expected context.arguments")?;
        assert_eq!(args.get("owner").map(String::as_str), Some("acme"));

        // A request without context must not emit the key.
        let bare = CompleteRequest {
            ref_: CompletionRef::prompt("p"),
            argument: CompletionArgument {
                name: "a".to_string(),
                value: "v".to_string(),
            },
            context: None,
        };
        assert!(serde_json::to_value(&bare)?.get("context").is_none());
        Ok(())
    }

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
            context: None,
        };

        let json = serde_json::to_string(&request)?;
        assert!(json.contains("ref/prompt"));

        let parsed: CompleteRequest = serde_json::from_str(&json)?;
        assert_eq!(parsed.argument.name, "arg1");
        Ok(())
    }
}
