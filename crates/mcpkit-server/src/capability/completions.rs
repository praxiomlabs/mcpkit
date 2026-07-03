//! Completion capability implementation.
//!
//! This module provides support for argument completion
//! in MCP servers.

use crate::context::Context;
use crate::handler::CompletionHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::completion::{CompleteRequest, Completion, CompletionRef};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// A boxed async function for handling completion requests.
pub type BoxedCompletionFn = Box<
    dyn for<'a> Fn(
            &'a str,
            &'a Context<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<Vec<String>, McpError>> + Send + 'a>>
        + Send
        + Sync,
>;

/// A registered completion provider.
pub struct RegisteredCompletion {
    /// Reference type (prompt or resource).
    pub ref_type: String,
    /// Reference value (name or URI pattern).
    pub ref_value: String,
    /// Argument name.
    pub arg_name: String,
    /// Completion handler.
    pub handler: BoxedCompletionFn,
}

/// Service for handling completion requests.
///
/// This provides argument completion for prompts and resources.
pub struct CompletionService {
    /// Completions keyed by (`ref_type`, `ref_value`, `arg_name`).
    completions: HashMap<(String, String, String), RegisteredCompletion>,
}

impl Default for CompletionService {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionService {
    /// Create a new completion service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            completions: HashMap::new(),
        }
    }

    /// Register a completion provider for a prompt argument.
    pub fn register_prompt_completion<F, Fut>(
        &mut self,
        prompt_name: impl Into<String>,
        arg_name: impl Into<String>,
        handler: F,
    ) where
        F: Fn(&str, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<String>, McpError>> + Send + 'static,
    {
        let ref_type = "ref/prompt".to_string();
        let ref_value = prompt_name.into();
        let arg_name = arg_name.into();
        let key = (ref_type.clone(), ref_value.clone(), arg_name.clone());

        let boxed: BoxedCompletionFn = Box::new(move |input, ctx| Box::pin(handler(input, ctx)));

        self.completions.insert(
            key,
            RegisteredCompletion {
                ref_type,
                ref_value,
                arg_name,
                handler: boxed,
            },
        );
    }

    /// Register a completion provider for a resource argument.
    pub fn register_resource_completion<F, Fut>(
        &mut self,
        uri_pattern: impl Into<String>,
        arg_name: impl Into<String>,
        handler: F,
    ) where
        F: Fn(&str, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<String>, McpError>> + Send + 'static,
    {
        let ref_type = "ref/resource".to_string();
        let ref_value = uri_pattern.into();
        let arg_name = arg_name.into();
        let key = (ref_type.clone(), ref_value.clone(), arg_name.clone());

        let boxed: BoxedCompletionFn = Box::new(move |input, ctx| Box::pin(handler(input, ctx)));

        self.completions.insert(
            key,
            RegisteredCompletion {
                ref_type,
                ref_value,
                arg_name,
                handler: boxed,
            },
        );
    }

    /// Check if a completion provider exists.
    #[must_use]
    pub fn has_completion(&self, ref_type: &str, ref_value: &str, arg_name: &str) -> bool {
        let key = (
            ref_type.to_string(),
            ref_value.to_string(),
            arg_name.to_string(),
        );
        self.completions.contains_key(&key)
    }
}

impl CompletionHandler for CompletionService {
    async fn complete(
        &self,
        request: &CompleteRequest,
        ctx: &Context<'_>,
    ) -> Result<Completion, McpError> {
        // Dispatch to the registered closure for this (ref, argument). Closures
        // return the matching values; `total`/`has_more` are derived here (a
        // handler that needs a real superset total or paging should implement
        // `CompletionHandler` directly rather than via the closure registry).
        let key = (
            request.ref_.ref_type().to_string(),
            request.ref_.value().to_string(),
            request.argument.name.clone(),
        );

        let Some(registered) = self.completions.get(&key) else {
            return Ok(Completion::new(Vec::new()));
        };
        let values = (registered.handler)(&request.argument.value, ctx).await?;
        Ok(Completion::new(values))
    }
}

/// Builder for creating completion requests.
pub struct CompleteRequestBuilder {
    ref_: CompletionRef,
    arg_name: String,
    arg_value: String,
}

impl CompleteRequestBuilder {
    /// Create a builder for prompt argument completion.
    pub fn for_prompt(prompt_name: impl Into<String>, arg_name: impl Into<String>) -> Self {
        Self {
            ref_: CompletionRef::prompt(prompt_name.into()),
            arg_name: arg_name.into(),
            arg_value: String::new(),
        }
    }

    /// Create a builder for resource argument completion.
    pub fn for_resource(uri: impl Into<String>, arg_name: impl Into<String>) -> Self {
        Self {
            ref_: CompletionRef::resource(uri.into()),
            arg_name: arg_name.into(),
            arg_value: String::new(),
        }
    }

    /// Set the current input value.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.arg_value = value.into();
        self
    }

    /// Build the request.
    #[must_use]
    pub fn build(self) -> CompleteRequest {
        CompleteRequest {
            ref_: self.ref_,
            argument: mcpkit_core::types::completion::CompletionArgument {
                name: self.arg_name,
                value: self.arg_value,
            },
            context: None,
        }
    }
}

/// Helper for filtering completions.
pub struct CompletionFilter;

impl CompletionFilter {
    /// Filter completions by prefix match.
    #[must_use]
    pub fn by_prefix(values: &[String], prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        values
            .iter()
            .filter(|v| v.to_lowercase().starts_with(&prefix_lower))
            .cloned()
            .collect()
    }

    /// Filter completions by substring match.
    #[must_use]
    pub fn by_substring(values: &[String], substring: &str) -> Vec<String> {
        let sub_lower = substring.to_lowercase();
        values
            .iter()
            .filter(|v| v.to_lowercase().contains(&sub_lower))
            .cloned()
            .collect()
    }

    /// Filter and limit completions.
    #[must_use]
    pub fn limit(values: Vec<String>, max: usize) -> Vec<String> {
        values.into_iter().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Context, NoOpPeer};
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;

    fn make_context() -> (
        RequestId,
        ClientCapabilities,
        ServerCapabilities,
        ProtocolVersion,
        NoOpPeer,
    ) {
        (
            RequestId::Number(1),
            ClientCapabilities::default(),
            ServerCapabilities::default(),
            ProtocolVersion::LATEST,
            NoOpPeer,
        )
    }

    #[test]
    fn test_complete_request_builder() {
        let request = CompleteRequestBuilder::for_prompt("code-review", "language")
            .value("py")
            .build();

        assert_eq!(request.ref_.ref_type(), "ref/prompt");
        assert_eq!(request.argument.name, "language");
        assert_eq!(request.argument.value, "py");
    }

    #[test]
    fn test_completion_filter() {
        let values = vec![
            "python".to_string(),
            "javascript".to_string(),
            "typescript".to_string(),
            "rust".to_string(),
        ];

        let filtered = CompletionFilter::by_prefix(&values, "py");
        assert_eq!(filtered, vec!["python"]);

        let filtered = CompletionFilter::by_substring(&values, "script");
        assert_eq!(filtered, vec!["javascript", "typescript"]);

        let limited = CompletionFilter::limit(values, 2);
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn test_completion_service() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = CompletionService::new();

        let languages = vec![
            "python".to_string(),
            "javascript".to_string(),
            "typescript".to_string(),
            "rust".to_string(),
        ];

        service.register_prompt_completion("code-review", "language", move |input, _ctx| {
            let langs = languages.clone();
            let input = input.to_string();
            async move { Ok(CompletionFilter::by_prefix(&langs, &input)) }
        });

        assert!(service.has_completion("ref/prompt", "code-review", "language"));

        let (req_id, client_caps, server_caps, protocol_version, peer) = make_context();
        let ctx = Context::new(
            &req_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
        );

        let request = CompleteRequestBuilder::for_prompt("code-review", "language")
            .value("py")
            .build();

        let completion = service.complete(&request, &ctx).await?;
        assert_eq!(completion.values, vec!["python"]);

        Ok(())
    }
}
