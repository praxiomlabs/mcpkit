//! Sampling capability implementation.
//!
//! This module provides support for LLM sampling requests
//! in MCP servers.

use crate::context::Context;
use mcpkit_core::error::McpError;
use mcpkit_core::types::content::Role;
use mcpkit_core::types::sampling::{
    CreateMessageRequest, CreateMessageResult, IncludeContext, ModelPreferences, SamplingMessage,
    StopReason,
};
use std::future::Future;
use std::pin::Pin;

/// A boxed async function for handling sampling requests.
pub type BoxedSamplingFn = Box<
    dyn for<'a> Fn(
            CreateMessageRequest,
            &'a Context<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<CreateMessageResult, McpError>> + Send + 'a>,
        > + Send
        + Sync,
>;

/// Service for handling sampling requests.
///
/// This allows clients to request LLM completions through the server.
pub struct SamplingService {
    handler: Option<BoxedSamplingFn>,
}

impl Default for SamplingService {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplingService {
    /// Create a new sampling service without a handler.
    #[must_use]
    pub fn new() -> Self {
        Self { handler: None }
    }

    /// Set the sampling handler.
    pub fn with_handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(CreateMessageRequest, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<CreateMessageResult, McpError>> + Send + 'static,
    {
        self.handler = Some(Box::new(move |req, ctx| Box::pin(handler(req, ctx))));
        self
    }

    /// Check if sampling is supported.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        self.handler.is_some()
    }

    /// Create a message (perform sampling).
    pub async fn create_message(
        &self,
        request: CreateMessageRequest,
        ctx: &Context<'_>,
    ) -> Result<CreateMessageResult, McpError> {
        let handler = self
            .handler
            .as_ref()
            .ok_or_else(|| McpError::invalid_request("Sampling not supported"))?;

        (handler)(request, ctx).await
    }
}

/// Builder for creating sampling requests.
pub struct SamplingRequestBuilder {
    messages: Vec<SamplingMessage>,
    model_preferences: Option<ModelPreferences>,
    system_prompt: Option<String>,
    include_context: Option<IncludeContext>,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
    stop_sequences: Vec<String>,
}

impl Default for SamplingRequestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplingRequestBuilder {
    /// Create a new request builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            messages: Vec::new(),
            model_preferences: None,
            system_prompt: None,
            include_context: None,
            max_tokens: None,
            temperature: None,
            stop_sequences: Vec::new(),
        }
    }

    /// Add a user message.
    pub fn user(mut self, content: impl Into<String>) -> Self {
        self.messages.push(SamplingMessage::user(content.into()));
        self
    }

    /// Add an assistant message.
    pub fn assistant(mut self, content: impl Into<String>) -> Self {
        self.messages
            .push(SamplingMessage::assistant(content.into()));
        self
    }

    /// Add a message.
    #[must_use]
    pub fn message(mut self, msg: SamplingMessage) -> Self {
        self.messages.push(msg);
        self
    }

    /// Set model preferences.
    #[must_use]
    pub fn model_preferences(mut self, prefs: ModelPreferences) -> Self {
        self.model_preferences = Some(prefs);
        self
    }

    /// Set context inclusion.
    #[must_use]
    pub const fn include_context(mut self, context: IncludeContext) -> Self {
        self.include_context = Some(context);
        self
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the maximum tokens.
    #[must_use]
    pub const fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set the temperature.
    #[must_use]
    pub const fn temperature(mut self, temp: f64) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Add a stop sequence.
    pub fn stop_sequence(mut self, seq: impl Into<String>) -> Self {
        self.stop_sequences.push(seq.into());
        self
    }

    /// Build the request.
    #[must_use]
    pub fn build(self) -> CreateMessageRequest {
        CreateMessageRequest {
            messages: self.messages,
            model_preferences: self.model_preferences,
            system_prompt: self.system_prompt,
            include_context: self.include_context,
            max_tokens: self.max_tokens.unwrap_or(1024),
            temperature: self.temperature,
            stop_sequences: if self.stop_sequences.is_empty() {
                None
            } else {
                Some(self.stop_sequences)
            },
            metadata: None,
        }
    }
}

/// Builder for creating sampling results.
pub struct SamplingResultBuilder {
    role: Role,
    content: String,
    model: String,
    stop_reason: Option<StopReason>,
}

impl SamplingResultBuilder {
    /// Create a new result builder.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            model: model.into(),
            stop_reason: None,
        }
    }

    /// Set the content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Set the stop reason.
    #[must_use]
    pub const fn stop_reason(mut self, reason: StopReason) -> Self {
        self.stop_reason = Some(reason);
        self
    }

    /// Mark as stopped due to end turn.
    #[must_use]
    pub const fn end_turn(mut self) -> Self {
        self.stop_reason = Some(StopReason::EndTurn);
        self
    }

    /// Mark as stopped due to max tokens.
    #[must_use]
    pub const fn max_tokens_reached(mut self) -> Self {
        self.stop_reason = Some(StopReason::MaxTokens);
        self
    }

    /// Build the result.
    #[must_use]
    pub fn build(self) -> CreateMessageResult {
        CreateMessageResult {
            role: self.role,
            content: mcpkit_core::types::content::Content::text(self.content),
            model: self.model,
            stop_reason: self.stop_reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_request_builder() {
        let request = SamplingRequestBuilder::new()
            .system_prompt("You are a helpful assistant")
            .user("Hello!")
            .max_tokens(100)
            .temperature(0.7)
            .build();

        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.max_tokens, 100);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(
            request.system_prompt.as_deref(),
            Some("You are a helpful assistant")
        );
    }

    #[test]
    fn test_sampling_result_builder() {
        let result = SamplingResultBuilder::new("gpt-4")
            .content("Hello! How can I help you?")
            .end_turn()
            .build();

        assert_eq!(result.role, Role::Assistant);
        assert_eq!(result.model, "gpt-4");
        assert_eq!(result.stop_reason, Some(StopReason::EndTurn));
    }

    #[test]
    fn test_sampling_service_default() {
        let service = SamplingService::new();
        assert!(!service.is_supported());
    }

    #[tokio::test]
    async fn test_sampling_service_with_handler() {
        let service = SamplingService::new().with_handler(|_req, _ctx| async {
            Ok(SamplingResultBuilder::new("test-model")
                .content("Test response")
                .end_turn()
                .build())
        });

        assert!(service.is_supported());
    }
}
