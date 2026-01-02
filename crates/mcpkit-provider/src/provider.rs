//! The core Provider trait and related types.
//!
//! This module defines the [`Provider`] trait that all LLM providers must implement,
//! as well as related types for provider capabilities and metadata.

use async_trait::async_trait;

use crate::error::{ProviderError, ProviderResult};
use crate::streaming::CompletionStream;
use crate::types::{CompletionRequest, CompletionResponse, ModelInfo};

/// Capabilities that a provider may support.
#[derive(Debug, Clone, Default)]
pub struct ProviderCapabilities {
    /// Whether the provider supports streaming responses.
    pub streaming: bool,
    /// Whether the provider supports tool/function calling.
    pub tools: bool,
    /// Whether the provider supports vision (image inputs).
    pub vision: bool,
    /// Whether the provider supports JSON mode.
    pub json_mode: bool,
    /// Whether the provider supports embeddings.
    pub embeddings: bool,
    /// Whether the provider supports listing available models.
    pub list_models: bool,
}

impl ProviderCapabilities {
    /// Create capabilities with all features enabled.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            streaming: true,
            tools: true,
            vision: true,
            json_mode: true,
            embeddings: true,
            list_models: true,
        }
    }

    /// Create capabilities for a basic text completion provider.
    #[must_use]
    pub const fn basic() -> Self {
        Self {
            streaming: true,
            tools: false,
            vision: false,
            json_mode: false,
            embeddings: false,
            list_models: false,
        }
    }
}

/// Information about a provider.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// The provider's name (e.g., "openai", "anthropic").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// The provider's capabilities.
    pub capabilities: ProviderCapabilities,
    /// The default model to use.
    pub default_model: Option<String>,
    /// Base URL for the API.
    pub base_url: String,
}

impl ProviderInfo {
    /// Create a new provider info.
    #[must_use]
    pub fn new(name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            capabilities: ProviderCapabilities::default(),
            default_model: None,
            base_url: String::new(),
        }
    }

    /// Set the capabilities.
    #[must_use]
    pub fn capabilities(mut self, capabilities: ProviderCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Set the default model.
    #[must_use]
    pub fn default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    /// Set the base URL.
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

/// Metadata about a provider operation (for observability).
#[derive(Debug, Clone, Default)]
pub struct ProviderMetadata {
    /// The provider name.
    pub provider: String,
    /// The model used.
    pub model: String,
    /// Request latency in milliseconds.
    pub latency_ms: u64,
    /// Number of retry attempts.
    pub retry_count: u32,
    /// Whether the response was cached.
    pub cached: bool,
}

/// The core trait that all LLM providers must implement.
///
/// This trait provides a unified interface for interacting with any LLM provider,
/// enabling provider-agnostic code that can work with OpenAI, Anthropic, Ollama,
/// or any other compatible provider.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_provider::{Provider, CompletionRequest, Message};
///
/// async fn get_response(provider: &impl Provider) -> Result<String, ProviderError> {
///     let request = CompletionRequest::new()
///         .message(Message::user("Hello!"))
///         .max_tokens(100);
///
///     let response = provider.complete(request).await?;
///     Ok(response.text().unwrap_or_default())
/// }
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get information about this provider.
    fn info(&self) -> &ProviderInfo;

    /// Get the provider's name.
    fn name(&self) -> &str {
        &self.info().name
    }

    /// Get the provider's capabilities.
    fn capabilities(&self) -> &ProviderCapabilities {
        &self.info().capabilities
    }

    /// Complete a conversation (non-streaming).
    ///
    /// This is the primary method for generating completions. It takes a request
    /// containing messages and options, and returns a complete response.
    ///
    /// # Errors
    ///
    /// Returns an error if the completion fails (network error, invalid request,
    /// rate limit, etc.).
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse>;

    /// Complete a conversation with streaming.
    ///
    /// Returns a stream of events as the model generates tokens. This enables
    /// real-time display of responses.
    ///
    /// # Errors
    ///
    /// Returns an error if streaming is not supported or the request fails.
    async fn complete_stream(&self, request: CompletionRequest)
        -> ProviderResult<CompletionStream>;

    /// List available models.
    ///
    /// Returns information about models available from this provider.
    ///
    /// # Errors
    ///
    /// Returns an error if listing models is not supported or fails.
    async fn list_models(&self) -> ProviderResult<Vec<ModelInfo>>;

    /// Get information about a specific model.
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not found or the request fails.
    async fn get_model(&self, model_id: &str) -> ProviderResult<ModelInfo>;

    /// Generate embeddings for the given text(s).
    ///
    /// # Errors
    ///
    /// Returns an error if embeddings are not supported or the request fails.
    async fn embed(
        &self,
        _request: crate::types::EmbeddingRequest,
    ) -> ProviderResult<crate::types::EmbeddingResponse> {
        Err(ProviderError::Unsupported {
            provider: self.name().to_string(),
            feature: "embeddings".to_string(),
        })
    }

    /// Check if this provider supports a specific capability.
    fn supports(&self, capability: &str) -> bool {
        let caps = self.capabilities();
        match capability {
            "streaming" => caps.streaming,
            "tools" => caps.tools,
            "vision" => caps.vision,
            "json_mode" => caps.json_mode,
            "embeddings" => caps.embeddings,
            "list_models" => caps.list_models,
            _ => false,
        }
    }
}

/// Extension trait for providers with additional utility methods.
#[async_trait]
pub trait ProviderExt: Provider {
    /// Complete with automatic retries on retryable errors.
    ///
    /// Uses the provided retry config to handle transient failures.
    async fn complete_with_retry(
        &self,
        request: CompletionRequest,
        config: &crate::retry::RetryConfig,
    ) -> ProviderResult<CompletionResponse> {
        let mut attempts = 0;

        loop {
            match self.complete(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) if e.is_retryable() && attempts < config.max_retries => {
                    attempts += 1;
                    let delay = config.delay_for_attempt(attempts);
                    if let Some(retry_after) = e.retry_after() {
                        tokio::time::sleep(retry_after.max(delay)).await;
                    } else {
                        tokio::time::sleep(delay).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Complete a simple text prompt.
    ///
    /// Convenience method for simple single-turn completions.
    async fn complete_text(&self, prompt: &str) -> ProviderResult<String> {
        let request = CompletionRequest::new().message(crate::types::Message::user(prompt));
        let response = self.complete(request).await?;
        Ok(response.text().unwrap_or_default())
    }
}

// Blanket implementation for all providers
impl<T: Provider> ProviderExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities() {
        let caps = ProviderCapabilities::full();
        assert!(caps.streaming);
        assert!(caps.tools);
        assert!(caps.vision);

        let basic = ProviderCapabilities::basic();
        assert!(basic.streaming);
        assert!(!basic.tools);
        assert!(!basic.vision);
    }

    #[test]
    fn test_provider_info() {
        let info = ProviderInfo::new("openai", "OpenAI")
            .capabilities(ProviderCapabilities::full())
            .default_model("gpt-4")
            .base_url("https://api.openai.com/v1");

        assert_eq!(info.name, "openai");
        assert_eq!(info.display_name, "OpenAI");
        assert_eq!(info.default_model, Some("gpt-4".to_string()));
        assert!(info.capabilities.tools);
    }
}
