//! # mcpkit-provider
//!
//! Multi-LLM provider abstraction for the mcpkit-forge orchestration layer.
//!
//! This crate provides a unified interface for interacting with various LLM providers
//! (OpenAI, Anthropic, Ollama, etc.) with support for:
//!
//! - **Streaming completions**: Token-by-token streaming with backpressure
//! - **Tool/function calling**: Unified interface across providers
//! - **Retry policies**: Configurable retry and fallback strategies
//! - **Cost tracking**: Token usage and cost estimation
//! - **Rate limiting**: Built-in rate limit handling
//!
//! # Architecture
//!
//! The crate is built around the [`Provider`] trait, which defines the core interface
//! that all LLM providers implement. This enables:
//!
//! - Swapping providers without code changes
//! - Fallback chains across providers
//! - A/B testing between providers
//! - Cost optimization through provider selection
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_provider::{Provider, Message, CompletionRequest};
//! use mcpkit_provider::openai::OpenAiProvider;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OpenAiProvider::new("your-api-key")?;
//!
//!     let request = CompletionRequest::new()
//!         .model("gpt-4")
//!         .messages(vec![
//!             Message::user("What is the capital of France?")
//!         ]);
//!
//!     let response = provider.complete(request).await?;
//!     println!("Response: {}", response.content);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Feature Flags
//!
//! - `openai` (default): Enable OpenAI provider
//! - `anthropic` (default): Enable Anthropic provider
//! - `ollama`: Enable Ollama provider for local models
//! - `all-providers`: Enable all providers

#![deny(missing_docs)]

mod error;
mod provider;
mod types;

pub mod rate_limit;
pub mod retry;
pub mod streaming;

// Provider implementations (feature-gated)
#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "ollama")]
pub mod ollama;

// Re-export core types
pub use error::{ProviderError, ProviderResult};
pub use provider::{Provider, ProviderCapabilities, ProviderExt, ProviderInfo, ProviderMetadata};
pub use types::{
    CompletionRequest, CompletionResponse, ContentBlock, ContentDelta, Embedding, EmbeddingRequest,
    EmbeddingResponse, EmbeddingUsage, FinishReason, FunctionCall, FunctionDefinition, Message,
    MessageContent, ModelInfo, ResponseFormat, Role, StreamEvent, ToolCall, ToolChoice,
    ToolDefinition, ToolResult, Usage,
};

// Re-export streaming types
pub use streaming::{CompletionStream, StreamState};

// Re-export retry/rate limit types
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use retry::{RetryConfig, RetryPolicy};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::{ProviderError, ProviderResult};
    pub use crate::provider::{Provider, ProviderCapabilities, ProviderInfo};
    pub use crate::types::{
        CompletionRequest, CompletionResponse, FinishReason, Message, Role, ToolCall,
        ToolDefinition, Usage,
    };

    #[cfg(feature = "openai")]
    pub use crate::openai::OpenAiProvider;

    #[cfg(feature = "anthropic")]
    pub use crate::anthropic::AnthropicProvider;

    #[cfg(feature = "ollama")]
    pub use crate::ollama::OllamaProvider;
}
