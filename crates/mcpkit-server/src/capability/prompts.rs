//! Prompt capability implementation.
//!
//! This module provides utilities for managing and rendering prompts
//! in an MCP server.

use crate::context::Context;
use crate::handler::PromptHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::prompt::{GetPromptResult, Prompt, PromptArgument, PromptMessage};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// A boxed async function for prompt rendering.
pub type BoxedPromptFn = Box<
    dyn for<'a> Fn(
            Option<Value>,
            &'a Context<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<GetPromptResult, McpError>> + Send + 'a>>
        + Send
        + Sync,
>;

/// A registered prompt with metadata and handler.
pub struct RegisteredPrompt {
    /// Prompt metadata.
    pub prompt: Prompt,
    /// Handler function for rendering.
    pub handler: BoxedPromptFn,
}

/// Service for managing prompts.
///
/// This provides a registry for prompts and handles rendering
/// them with arguments.
pub struct PromptService {
    prompts: HashMap<String, RegisteredPrompt>,
}

impl Default for PromptService {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptService {
    /// Create a new empty prompt service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
        }
    }

    /// Register a prompt with a handler function.
    pub fn register<F, Fut>(&mut self, prompt: Prompt, handler: F)
    where
        F: Fn(Option<Value>, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<GetPromptResult, McpError>> + Send + 'static,
    {
        let name = prompt.name.clone();
        let boxed: BoxedPromptFn = Box::new(move |args, ctx| Box::pin(handler(args, ctx)));
        self.prompts.insert(
            name,
            RegisteredPrompt {
                prompt,
                handler: boxed,
            },
        );
    }

    /// Get a prompt by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RegisteredPrompt> {
        self.prompts.get(name)
    }

    /// Check if a prompt exists.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.prompts.contains_key(name)
    }

    /// List all registered prompts.
    #[must_use]
    pub fn list(&self) -> Vec<&Prompt> {
        self.prompts.values().map(|r| &r.prompt).collect()
    }

    /// Get the number of registered prompts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prompts.len()
    }

    /// Check if the service has no prompts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }

    /// Render a prompt by name with arguments.
    pub async fn render(
        &self,
        name: &str,
        arguments: Option<Value>,
        ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        let registered = self.prompts.get(name).ok_or_else(|| {
            McpError::invalid_params("prompts/get", format!("Unknown prompt: {name}"))
        })?;

        (registered.handler)(arguments, ctx).await
    }
}

impl PromptHandler for PromptService {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(self.list().into_iter().cloned().collect())
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        let args = arguments.map(Value::Object);
        self.render(name, args, ctx).await
    }
}

/// Builder for creating prompts with a fluent API.
pub struct PromptBuilder {
    name: String,
    description: Option<String>,
    arguments: Vec<PromptArgument>,
}

impl PromptBuilder {
    /// Create a new prompt builder.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            arguments: Vec::new(),
        }
    }

    /// Set the prompt description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a required argument.
    pub fn required_arg(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.arguments.push(PromptArgument {
            name: name.into(),
            description: Some(description.into()),
            required: Some(true),
        });
        self
    }

    /// Add an optional argument.
    pub fn optional_arg(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.arguments.push(PromptArgument {
            name: name.into(),
            description: Some(description.into()),
            required: Some(false),
        });
        self
    }

    /// Add a custom argument.
    #[must_use]
    pub fn argument(mut self, arg: PromptArgument) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Build the prompt.
    #[must_use]
    pub fn build(self) -> Prompt {
        Prompt {
            name: self.name,
            description: self.description,
            arguments: if self.arguments.is_empty() {
                None
            } else {
                Some(self.arguments)
            },
        }
    }
}

/// Builder for creating prompt results.
pub struct PromptResultBuilder {
    description: Option<String>,
    messages: Vec<PromptMessage>,
}

impl Default for PromptResultBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptResultBuilder {
    /// Create a new result builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            description: None,
            messages: Vec::new(),
        }
    }

    /// Set the result description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a user message with text content.
    pub fn user_text(mut self, text: impl Into<String>) -> Self {
        self.messages.push(PromptMessage::user(text.into()));
        self
    }

    /// Add an assistant message with text content.
    pub fn assistant_text(mut self, text: impl Into<String>) -> Self {
        self.messages.push(PromptMessage::assistant(text.into()));
        self
    }

    /// Add a custom message.
    #[must_use]
    pub fn message(mut self, msg: PromptMessage) -> Self {
        self.messages.push(msg);
        self
    }

    /// Build the result.
    #[must_use]
    pub fn build(self) -> GetPromptResult {
        GetPromptResult {
            description: self.description,
            messages: self.messages,
        }
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
    fn test_prompt_builder() {
        let prompt = PromptBuilder::new("code-review")
            .description("Review code for issues")
            .required_arg("code", "The code to review")
            .optional_arg("language", "Programming language")
            .build();

        assert_eq!(prompt.name, "code-review");
        assert_eq!(
            prompt.description.as_deref(),
            Some("Review code for issues")
        );
        assert_eq!(prompt.arguments.as_ref().map(std::vec::Vec::len), Some(2));
    }

    #[test]
    fn test_prompt_result_builder() {
        let result = PromptResultBuilder::new()
            .description("Generated review")
            .user_text("Please review this code")
            .assistant_text("I'll analyze the code...")
            .build();

        assert_eq!(result.description.as_deref(), Some("Generated review"));
        assert_eq!(result.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_prompt_service() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = PromptService::new();

        let prompt = PromptBuilder::new("greeting")
            .description("Generate a greeting")
            .required_arg("name", "Name to greet")
            .build();

        service.register(prompt, |args, _ctx| async move {
            let name = args
                .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                .unwrap_or_else(|| "World".to_string());

            Ok(PromptResultBuilder::new()
                .user_text(format!("Generate a greeting for {name}"))
                .build())
        });

        assert!(service.contains("greeting"));
        assert_eq!(service.len(), 1);

        let (req_id, client_caps, server_caps, protocol_version, peer) = make_context();
        let ctx = Context::new(
            &req_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
        );

        let result = service
            .render("greeting", Some(serde_json::json!({"name": "Alice"})), &ctx)
            .await?;

        assert!(!result.messages.is_empty());

        Ok(())
    }
}
