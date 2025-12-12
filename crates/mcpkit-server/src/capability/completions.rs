//! Completion capability implementation.
//!
//! This module provides support for argument completion
//! in MCP servers.

use crate::context::Context;
use crate::handler::CompletionHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::completion::{
    CompleteRequest, CompleteResult, Completion, CompletionArgument, CompletionRef, CompletionTotal,
};
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

    /// Complete an argument value.
    pub async fn complete(
        &self,
        request: &CompleteRequest,
        ctx: &Context<'_>,
    ) -> Result<CompleteResult, McpError> {
        let ref_type = request.ref_.ref_type().to_string();
        let ref_value = request.ref_.value().to_string();
        let arg_name = request.argument.name.clone();
        let input = &request.argument.value;

        let key = (ref_type, ref_value, arg_name);

        if let Some(registered) = self.completions.get(&key) {
            let values = (registered.handler)(input, ctx).await?;
            let total = values.len();

            Ok(CompleteResult {
                completion: Completion {
                    values,
                    total: Some(CompletionTotal::Exact(total)),
                    has_more: Some(false),
                },
            })
        } else {
            // Return empty completions if no handler registered
            Ok(CompleteResult {
                completion: Completion {
                    values: Vec::new(),
                    total: Some(CompletionTotal::Exact(0)),
                    has_more: Some(false),
                },
            })
        }
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
    async fn complete_resource(
        &self,
        partial_uri: &str,
        ctx: &Context<'_>,
    ) -> Result<Vec<String>, McpError> {
        let request = CompleteRequest {
            ref_: CompletionRef::resource(partial_uri),
            argument: CompletionArgument {
                name: "uri".to_string(),
                value: partial_uri.to_string(),
            },
        };

        let result = Self::complete(self, &request, ctx).await?;
        Ok(result.completion.values)
    }

    async fn complete_prompt_arg(
        &self,
        prompt_name: &str,
        arg_name: &str,
        partial_value: &str,
        ctx: &Context<'_>,
    ) -> Result<Vec<String>, McpError> {
        let request = CompleteRequest {
            ref_: CompletionRef::prompt(prompt_name),
            argument: CompletionArgument {
                name: arg_name.to_string(),
                value: partial_value.to_string(),
            },
        };

        let result = Self::complete(self, &request, ctx).await?;
        Ok(result.completion.values)
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

    fn make_context() -> (RequestId, ClientCapabilities, ServerCapabilities, NoOpPeer) {
        (
            RequestId::Number(1),
            ClientCapabilities::default(),
            ServerCapabilities::default(),
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
    async fn test_completion_service() {
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

        let (req_id, client_caps, server_caps, peer) = make_context();
        let ctx = Context::new(&req_id, None, &client_caps, &server_caps, &peer);

        let request = CompleteRequestBuilder::for_prompt("code-review", "language")
            .value("py")
            .build();

        let result = service.complete(&request, &ctx).await.unwrap();
        assert_eq!(result.completion.values, vec!["python"]);
    }
}
