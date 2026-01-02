//! Anthropic provider implementation.
//!
//! This module provides an implementation of the [`Provider`] trait for Anthropic's API,
//! supporting Claude models including Claude 3.5 Sonnet, Claude 3 Opus, and others.
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_provider::anthropic::AnthropicProvider;
//! use mcpkit_provider::{Provider, CompletionRequest, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = AnthropicProvider::new("your-api-key")?;
//!
//!     let request = CompletionRequest::new()
//!         .model("claude-3-5-sonnet-20241022")
//!         .message(Message::user("Hello!"))
//!         .max_tokens(1024);
//!
//!     let response = provider.complete(request).await?;
//!     println!("Response: {}", response.text().unwrap_or_default());
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, instrument};

use crate::error::{ProviderError, ProviderResult};
use crate::provider::{Provider, ProviderCapabilities, ProviderInfo};
use crate::streaming::CompletionStream;
use crate::types::{
    CompletionRequest, CompletionResponse, ContentBlock, FinishReason, Message, MessageContent,
    ModelInfo, Role, StreamEvent, ToolDefinition, Usage,
};

/// Default Anthropic API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Default model to use if none specified.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Current Anthropic API version.
pub const API_VERSION: &str = "2023-06-01";

/// Configuration for the Anthropic provider.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Base URL for the API.
    pub base_url: String,
    /// Default model to use.
    pub default_model: String,
    /// Request timeout.
    pub timeout: Duration,
    /// Default max tokens (Anthropic requires this).
    pub default_max_tokens: u32,
}

impl AnthropicConfig {
    /// Create a new config with the given API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(120),
            default_max_tokens: 4096,
        }
    }

    /// Set the base URL.
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model.
    #[must_use]
    pub fn default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the default max tokens.
    #[must_use]
    pub const fn default_max_tokens(mut self, tokens: u32) -> Self {
        self.default_max_tokens = tokens;
        self
    }
}

/// Anthropic provider implementation.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: reqwest::Client,
    info: ProviderInfo,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(api_key: impl Into<String>) -> ProviderResult<Self> {
        Self::with_config(AnthropicConfig::new(api_key))
    }

    /// Create a new Anthropic provider with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn with_config(config: AnthropicConfig) -> ProviderResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&config.api_key).map_err(|_| ProviderError::Configuration {
                message: "Invalid API key format".to_string(),
            })?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .map_err(|e| ProviderError::Configuration {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let info = ProviderInfo::new("anthropic", "Anthropic")
            .capabilities(ProviderCapabilities {
                streaming: true,
                tools: true,
                vision: true,
                json_mode: false, // Anthropic doesn't have native JSON mode
                embeddings: false,
                list_models: false,
            })
            .default_model(config.default_model.clone())
            .base_url(config.base_url.clone());

        Ok(Self {
            config,
            client,
            info,
        })
    }

    /// Get the effective model for a request.
    fn effective_model<'a>(&'a self, request: &'a CompletionRequest) -> &'a str {
        request
            .model
            .as_deref()
            .unwrap_or(&self.config.default_model)
    }

    /// Convert our messages to Anthropic's format.
    fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<AnthropicMessage>) {
        let mut system = None;
        let mut anthropic_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    // Anthropic expects system as a separate field
                    if let Some(text) = msg.text() {
                        system = Some(text.to_string());
                    }
                }
                Role::User | Role::Assistant | Role::Tool => {
                    let role = match msg.role {
                        Role::User | Role::Tool => "user",
                        Role::Assistant => "assistant",
                        _ => continue,
                    };

                    let content: Vec<AnthropicContent> = msg
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            MessageContent::Text { text } => {
                                Some(AnthropicContent::Text { text: text.clone() })
                            }
                            MessageContent::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => Some(AnthropicContent::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: content.clone(),
                                is_error: Some(*is_error),
                            }),
                            MessageContent::Image { source } => {
                                // Convert image source - supports both base64 and URL
                                let anthropic_source = match source {
                                    crate::types::ImageSource::Base64 { media_type, data } => {
                                        AnthropicImageSource::Base64 {
                                            media_type: media_type.clone(),
                                            data: data.clone(),
                                        }
                                    }
                                    crate::types::ImageSource::Url { url } => {
                                        AnthropicImageSource::Url { url: url.clone() }
                                    }
                                };
                                Some(AnthropicContent::Image {
                                    source: anthropic_source,
                                })
                            }
                            _ => None,
                        })
                        .collect();

                    if !content.is_empty() {
                        anthropic_messages.push(AnthropicMessage {
                            role: role.to_string(),
                            content,
                        });
                    }
                }
            }
        }

        (system, anthropic_messages)
    }

    /// Convert our tool definitions to Anthropic's format.
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    /// Parse an Anthropic error response.
    fn parse_error(&self, status: reqwest::StatusCode, body: &str) -> ProviderError {
        if let Ok(error) = serde_json::from_str::<AnthropicErrorResponse>(body) {
            let message = error.error.message;
            let error_type = error.error.error_type;

            return match error_type.as_str() {
                "authentication_error" => ProviderError::auth_failed("anthropic", message),
                "rate_limit_error" => ProviderError::rate_limited("anthropic", None),
                "invalid_request_error" if message.contains("context length") => {
                    ProviderError::ContextLengthExceeded {
                        message,
                        max_tokens: None,
                        actual_tokens: None,
                    }
                }
                _ => ProviderError::Other {
                    provider: "anthropic".to_string(),
                    message,
                    code: Some(error_type),
                },
            };
        }

        ProviderError::UnexpectedResponse {
            provider: "anthropic".to_string(),
            message: format!("HTTP {status}: {body}"),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    #[instrument(skip(self, request), fields(model = %self.effective_model(&request)))]
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let model = self.effective_model(&request).to_string();
        let (system, messages) = Self::convert_messages(&request.messages);

        let anthropic_request = AnthropicMessagesRequest {
            model: model.clone(),
            messages,
            system,
            max_tokens: request.max_tokens.unwrap_or(self.config.default_max_tokens),
            temperature: request.temperature,
            top_p: request.top_p,
            stop_sequences: request.stop.clone(),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            stream: false,
        };

        debug!("Sending request to Anthropic");

        let response = self
            .client
            .post(format!("{}/v1/messages", self.config.base_url))
            .json(&anthropic_request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(self.parse_error(status, &body));
        }

        let anthropic_response: AnthropicMessagesResponse =
            serde_json::from_str(&body).map_err(|e| ProviderError::UnexpectedResponse {
                provider: "anthropic".to_string(),
                message: format!("Failed to parse response: {e}"),
            })?;

        // Convert Anthropic response to our format
        let content: Vec<ContentBlock> = anthropic_response
            .content
            .into_iter()
            .filter_map(|c| match c {
                AnthropicContent::Text { text } => Some(ContentBlock::text(text)),
                AnthropicContent::ToolUse { id, name, input } => {
                    Some(ContentBlock::ToolUse { id, name, input })
                }
                _ => None,
            })
            .collect();

        let finish_reason = match anthropic_response.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            Some("tool_use") => FinishReason::ToolUse,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            id: anthropic_response.id,
            model: anthropic_response.model,
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: anthropic_response.usage.input_tokens,
                completion_tokens: anthropic_response.usage.output_tokens,
                total_tokens: anthropic_response.usage.input_tokens
                    + anthropic_response.usage.output_tokens,
                cached_tokens: anthropic_response.usage.cache_read_input_tokens,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> ProviderResult<CompletionStream> {
        let model = self.effective_model(&request).to_string();
        let (system, messages) = Self::convert_messages(&request.messages);

        let anthropic_request = AnthropicMessagesRequest {
            model: model.clone(),
            messages,
            system,
            max_tokens: request.max_tokens.unwrap_or(self.config.default_max_tokens),
            temperature: request.temperature,
            top_p: request.top_p,
            stop_sequences: request.stop.clone(),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            stream: true,
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.config.base_url))
            .json(&anthropic_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(self.parse_error(status, &body));
        }

        let model_clone = model.clone();
        let stream = async_stream::stream! {
            use futures::StreamExt;

            let mut emitted_start = false;
            let bytes_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut usage = Usage::new();
            let mut final_finish_reason = FinishReason::Stop;

            tokio::pin!(bytes_stream);

            while let Some(chunk) = bytes_stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            // Handle event: and data: lines
                            if let Some(data) = line.strip_prefix("data: ") {
                                match serde_json::from_str::<AnthropicStreamEvent>(data) {
                                    Ok(event) => {
                                        match event.event_type.as_str() {
                                            "message_start" => {
                                                if let Some(message) = event.message {
                                                    if !emitted_start {
                                                        yield Ok(StreamEvent::Start {
                                                            id: message.id,
                                                            model: model_clone.clone(),
                                                        });
                                                        emitted_start = true;
                                                    }
                                                }
                                            }
                                            "content_block_start" => {
                                                // Handle tool use start
                                                if let Some(content_block) = event.content_block {
                                                    match content_block {
                                                        AnthropicStreamContentBlock::ToolUse { id, name } => {
                                                            yield Ok(StreamEvent::ToolUseStart {
                                                                index: event.index.unwrap_or(0) as usize,
                                                                id,
                                                                name,
                                                            });
                                                        }
                                                        AnthropicStreamContentBlock::Text { .. } => {
                                                            // Text blocks don't need special start handling
                                                        }
                                                    }
                                                }
                                            }
                                            "content_block_delta" => {
                                                if let Some(delta) = event.delta {
                                                    if let Some(text) = delta.text {
                                                        yield Ok(StreamEvent::ContentDelta {
                                                            index: event.index.unwrap_or(0) as usize,
                                                            delta: crate::types::ContentDelta::Text { text },
                                                        });
                                                    }
                                                    if let Some(partial) = delta.partial_json {
                                                        yield Ok(StreamEvent::ContentDelta {
                                                            index: event.index.unwrap_or(0) as usize,
                                                            delta: crate::types::ContentDelta::ToolInput {
                                                                partial_json: partial,
                                                            },
                                                        });
                                                    }
                                                }
                                            }
                                            "message_delta" => {
                                                if let Some(u) = event.usage {
                                                    usage.completion_tokens = u.output_tokens;
                                                }
                                                // Check for stop reason in delta
                                                if let Some(delta) = event.delta {
                                                    if let Some(reason) = delta.stop_reason {
                                                        final_finish_reason = match reason.as_str() {
                                                            "end_turn" => FinishReason::Stop,
                                                            "max_tokens" => FinishReason::Length,
                                                            "tool_use" => FinishReason::ToolUse,
                                                            _ => FinishReason::Stop,
                                                        };
                                                    }
                                                }
                                            }
                                            "message_stop" => {
                                                yield Ok(StreamEvent::Stop {
                                                    finish_reason: final_finish_reason,
                                                    usage: usage.clone(),
                                                });
                                            }
                                            "error" => {
                                                yield Ok(StreamEvent::Error {
                                                    message: "Anthropic streaming error".to_string(),
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(e) => {
                                        yield Ok(StreamEvent::Error {
                                            message: format!("Failed to parse SSE event: {}", e),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::StreamInterrupted {
                            message: e.to_string(),
                        });
                        break;
                    }
                }
            }
        };

        Ok(CompletionStream::new(stream))
    }

    async fn list_models(&self) -> ProviderResult<Vec<ModelInfo>> {
        // Anthropic doesn't have a models list endpoint, so we return hardcoded models
        Ok(vec![
            self.get_model("claude-sonnet-4-20250514").await?,
            self.get_model("claude-3-5-sonnet-20241022").await?,
            self.get_model("claude-3-5-haiku-20241022").await?,
            self.get_model("claude-3-opus-20240229").await?,
        ])
    }

    async fn get_model(&self, model_id: &str) -> ProviderResult<ModelInfo> {
        let model = match model_id {
            "claude-sonnet-4-20250514" => ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: Some("Claude Sonnet 4".to_string()),
                context_length: Some(200_000),
                max_output_tokens: Some(64_000),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(3.0),
                output_cost_per_million: Some(15.0),
            },
            "claude-3-5-sonnet-20241022" => ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: Some("Claude 3.5 Sonnet".to_string()),
                context_length: Some(200_000),
                max_output_tokens: Some(8_192),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(3.0),
                output_cost_per_million: Some(15.0),
            },
            "claude-3-5-haiku-20241022" => ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: Some("Claude 3.5 Haiku".to_string()),
                context_length: Some(200_000),
                max_output_tokens: Some(8_192),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(0.25),
                output_cost_per_million: Some(1.25),
            },
            "claude-3-opus-20240229" => ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: Some("Claude 3 Opus".to_string()),
                context_length: Some(200_000),
                max_output_tokens: Some(4_096),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(15.0),
                output_cost_per_million: Some(75.0),
            },
            _ => ModelInfo::new(model_id),
        };

        Ok(model)
    }
}

// Anthropic API types

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContent {
    Text {
        text: String,
    },
    Image {
        source: AnthropicImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Image source for Anthropic - supports both base64 and URL formats.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicImageSource {
    /// Base64-encoded image data.
    Base64 {
        /// The media type (e.g., "image/jpeg").
        media_type: String,
        /// The base64-encoded image data.
        data: String,
    },
    /// URL-referenced image.
    Url {
        /// The image URL.
        url: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    index: Option<u32>,
    message: Option<AnthropicStreamMessage>,
    delta: Option<AnthropicStreamDelta>,
    usage: Option<AnthropicUsage>,
    content_block: Option<AnthropicStreamContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    text: Option<String>,
    partial_json: Option<String>,
    stop_reason: Option<String>,
}

/// Content block types in streaming responses.
/// The `text` field is required for deserialization but not currently used.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicStreamContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String },
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorResponse {
    error: AnthropicError,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = AnthropicConfig::new("test-key")
            .base_url("https://custom.api.com")
            .default_model("claude-3-opus-20240229")
            .default_max_tokens(8192)
            .timeout(Duration::from_secs(60));

        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://custom.api.com");
        assert_eq!(config.default_model, "claude-3-opus-20240229");
        assert_eq!(config.default_max_tokens, 8192);
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let (system, anthropic_messages) = AnthropicProvider::convert_messages(&messages);

        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(anthropic_messages.len(), 2);
        assert_eq!(anthropic_messages[0].role, "user");
        assert_eq!(anthropic_messages[1].role, "assistant");
    }
}
