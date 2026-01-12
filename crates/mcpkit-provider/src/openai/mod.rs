//! OpenAI provider implementation.
//!
//! This module provides an implementation of the [`Provider`] trait for OpenAI's API,
//! supporting GPT-4, GPT-3.5-turbo, and other OpenAI models.
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_provider::openai::OpenAiProvider;
//! use mcpkit_provider::{Provider, CompletionRequest, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OpenAiProvider::new("your-api-key")?;
//!
//!     let request = CompletionRequest::new()
//!         .model("gpt-4")
//!         .message(Message::user("Hello!"));
//!
//!     let response = provider.complete(request).await?;
//!     println!("Response: {}", response.text().unwrap_or_default());
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, instrument};

use crate::error::{ProviderError, ProviderResult};
use crate::provider::{Provider, ProviderCapabilities, ProviderInfo};
use crate::streaming::CompletionStream;
use crate::types::{
    CompletionRequest, CompletionResponse, ContentBlock, FinishReason, Message, ModelInfo, Role,
    StreamEvent, ToolDefinition, Usage,
};

/// Default OpenAI API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Default model to use if none specified.
pub const DEFAULT_MODEL: &str = "gpt-4o";

/// Configuration for the OpenAI provider.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Base URL for the API.
    pub base_url: String,
    /// Organization ID (optional).
    pub organization: Option<String>,
    /// Default model to use.
    pub default_model: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl OpenAiConfig {
    /// Create a new config with the given API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            organization: None,
            default_model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(120),
        }
    }

    /// Set the base URL.
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the organization ID.
    #[must_use]
    pub fn organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
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
}

/// `OpenAI` provider implementation.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
    info: ProviderInfo,
}

impl OpenAiProvider {
    /// Create a new `OpenAI` provider with the given API key.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(api_key: impl Into<String>) -> ProviderResult<Self> {
        Self::with_config(OpenAiConfig::new(api_key))
    }

    /// Create a new `OpenAI` provider with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn with_config(config: OpenAiConfig) -> ProviderResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.api_key)).map_err(|_| {
                ProviderError::Configuration {
                    message: "Invalid API key format".to_string(),
                }
            })?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(org) = &config.organization {
            headers.insert(
                "OpenAI-Organization",
                HeaderValue::from_str(org).map_err(|_| ProviderError::Configuration {
                    message: "Invalid organization ID format".to_string(),
                })?,
            );
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .map_err(|e| ProviderError::Configuration {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let info = ProviderInfo::new("openai", "OpenAI")
            .capabilities(ProviderCapabilities {
                streaming: true,
                tools: true,
                vision: true,
                json_mode: true,
                embeddings: true,
                list_models: true,
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

    /// Convert our message format to `OpenAI`'s format.
    fn convert_messages(messages: &[Message]) -> Vec<OpenAiMessage> {
        messages.iter().map(OpenAiMessage::from_message).collect()
    }

    /// Convert our tool definitions to `OpenAI`'s format.
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<OpenAiTool> {
        tools
            .iter()
            .map(|t| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    }

    /// Convert our response format to `OpenAI`'s format.
    #[allow(clippy::single_option_map)] // Helper function called from multiple places
    fn convert_response_format(
        format: Option<&crate::types::ResponseFormat>,
    ) -> Option<OpenAiResponseFormat> {
        format.map(|f| match f {
            crate::types::ResponseFormat::Text => OpenAiResponseFormat::Text,
            crate::types::ResponseFormat::JsonObject => OpenAiResponseFormat::JsonObject,
            crate::types::ResponseFormat::JsonSchema { schema, strict } => {
                OpenAiResponseFormat::JsonSchema {
                    json_schema: OpenAiJsonSchema {
                        schema: schema.clone(),
                        strict: if *strict { Some(true) } else { None },
                    },
                }
            }
        })
    }

    /// Parse an `OpenAI` error response.
    fn parse_error(&self, status: reqwest::StatusCode, body: &str) -> ProviderError {
        // Try to parse as OpenAI error
        if let Ok(error) = serde_json::from_str::<OpenAiErrorResponse>(body) {
            let message = error.error.message;
            let code = error.error.code;

            return match status.as_u16() {
                401 => ProviderError::auth_failed("openai", message),
                429 => ProviderError::rate_limited("openai", None),
                400 if message.contains("context_length") => ProviderError::ContextLengthExceeded {
                    message,
                    max_tokens: None,
                    actual_tokens: None,
                },
                _ => ProviderError::Other {
                    provider: "openai".to_string(),
                    message,
                    code,
                },
            };
        }

        ProviderError::UnexpectedResponse {
            provider: "openai".to_string(),
            message: format!("HTTP {status}: {body}"),
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    #[instrument(skip(self, request), fields(model = %self.effective_model(&request)))]
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let model = self.effective_model(&request).to_string();

        let openai_request = OpenAiChatRequest {
            model: model.clone(),
            messages: Self::convert_messages(&request.messages),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            stop: request.stop.clone(),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            response_format: Self::convert_response_format(request.response_format.as_ref()),
            stream: false,
        };

        debug!("Sending request to OpenAI");

        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .json(&openai_request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(self.parse_error(status, &body));
        }

        let openai_response: OpenAiChatResponse =
            serde_json::from_str(&body).map_err(|e| ProviderError::UnexpectedResponse {
                provider: "openai".to_string(),
                message: format!("Failed to parse response: {e}"),
            })?;

        // Convert OpenAI response to our format
        let choice = openai_response.choices.into_iter().next().ok_or_else(|| {
            ProviderError::UnexpectedResponse {
                provider: "openai".to_string(),
                message: "No choices in response".to_string(),
            }
        })?;

        let content = self.convert_choice_to_content(&choice);
        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") => FinishReason::ToolUse,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            id: openai_response.id,
            model: openai_response.model,
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
                cached_tokens: None,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> ProviderResult<CompletionStream> {
        let model = self.effective_model(&request).to_string();

        let openai_request = OpenAiChatRequest {
            model: model.clone(),
            messages: Self::convert_messages(&request.messages),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
            stop: request.stop.clone(),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            response_format: Self::convert_response_format(request.response_format.as_ref()),
            stream: true,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .json(&openai_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(self.parse_error(status, &body));
        }

        // Create a stream from the SSE response
        let model_clone = model.clone();
        let stream = async_stream::stream! {
            use futures::StreamExt;
            use std::collections::HashSet;

            let mut emitted_start = false;
            let bytes_stream = response.bytes_stream();

            // Track which tool call indices we've seen to emit ToolUseStart only once
            let mut seen_tool_indices: HashSet<usize> = HashSet::new();
            let mut final_finish_reason = FinishReason::Stop;

            // Process SSE events
            let mut buffer = String::new();
            tokio::pin!(bytes_stream);

            while let Some(chunk) = bytes_stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() || line.starts_with(':') {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    yield Ok(StreamEvent::Stop {
                                        finish_reason: final_finish_reason,
                                        usage: Usage::new(),
                                    });
                                    break;
                                }

                                match serde_json::from_str::<OpenAiStreamChunk>(data) {
                                    Ok(chunk) => {
                                        if !emitted_start {
                                            yield Ok(StreamEvent::Start {
                                                id: chunk.id.clone(),
                                                model: model_clone.clone(),
                                            });
                                            emitted_start = true;
                                        }

                                        for choice in chunk.choices {
                                            // Update finish reason if provided
                                            if let Some(reason) = &choice.finish_reason {
                                                final_finish_reason = match reason.as_str() {
                                                    "stop" => FinishReason::Stop,
                                                    "length" => FinishReason::Length,
                                                    "tool_calls" => FinishReason::ToolUse,
                                                    "content_filter" => FinishReason::ContentFilter,
                                                    _ => FinishReason::Stop,
                                                };
                                            }

                                            // Handle text content
                                            if let Some(content) = choice.delta.content {
                                                yield Ok(StreamEvent::ContentDelta {
                                                    index: 0,
                                                    delta: crate::types::ContentDelta::Text {
                                                        text: content,
                                                    },
                                                });
                                            }

                                            // Handle streaming tool calls
                                            if let Some(tool_calls) = choice.delta.tool_calls {
                                                for tool_call in tool_calls {
                                                    // Content block index: text is at 0, tool calls start at 1
                                                    let content_index = tool_call.index + 1;

                                                    // Check if this is a new tool call (has id and name)
                                                    // Use insert() which returns true if the value was newly inserted
                                                    if let (Some(id), Some(func)) = (&tool_call.id, &tool_call.function) {
                                                        if let Some(name) = &func.name {
                                                            if seen_tool_indices.insert(tool_call.index) {
                                                                yield Ok(StreamEvent::ToolUseStart {
                                                                    index: content_index,
                                                                    id: id.clone(),
                                                                    name: name.clone(),
                                                                });
                                                            }
                                                        }
                                                    }

                                                    // Emit argument deltas
                                                    if let Some(func) = &tool_call.function {
                                                        if let Some(args) = &func.arguments {
                                                            if !args.is_empty() {
                                                                yield Ok(StreamEvent::ContentDelta {
                                                                    index: content_index,
                                                                    delta: crate::types::ContentDelta::ToolInput {
                                                                        partial_json: args.clone(),
                                                                    },
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        yield Ok(StreamEvent::Error {
                                            message: format!("Failed to parse SSE chunk: {e}"),
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
        let response = self
            .client
            .get(format!("{}/models", self.config.base_url))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(self.parse_error(status, &body));
        }

        let models_response: OpenAiModelsResponse =
            serde_json::from_str(&body).map_err(|e| ProviderError::UnexpectedResponse {
                provider: "openai".to_string(),
                message: format!("Failed to parse models response: {e}"),
            })?;

        Ok(models_response
            .data
            .into_iter()
            .filter(|m| m.id.starts_with("gpt"))
            .map(|m| ModelInfo::new(m.id))
            .collect())
    }

    async fn get_model(&self, model_id: &str) -> ProviderResult<ModelInfo> {
        // OpenAI doesn't have a get single model endpoint, so we use hardcoded info
        let model = match model_id {
            "gpt-4o" => ModelInfo {
                id: "gpt-4o".to_string(),
                name: Some("GPT-4o".to_string()),
                context_length: Some(128_000),
                max_output_tokens: Some(16_384),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(2.5),
                output_cost_per_million: Some(10.0),
            },
            "gpt-4-turbo" => ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: Some("GPT-4 Turbo".to_string()),
                context_length: Some(128_000),
                max_output_tokens: Some(4_096),
                supports_tools: true,
                supports_vision: true,
                input_cost_per_million: Some(10.0),
                output_cost_per_million: Some(30.0),
            },
            "gpt-3.5-turbo" => ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: Some("GPT-3.5 Turbo".to_string()),
                context_length: Some(16_385),
                max_output_tokens: Some(4_096),
                supports_tools: true,
                supports_vision: false,
                input_cost_per_million: Some(0.5),
                output_cost_per_million: Some(1.5),
            },
            _ => ModelInfo::new(model_id),
        };

        Ok(model)
    }

    #[instrument(skip(self, request), fields(model = %request.model.as_deref().unwrap_or("text-embedding-3-small")))]
    async fn embed(
        &self,
        request: crate::types::EmbeddingRequest,
    ) -> ProviderResult<crate::types::EmbeddingResponse> {
        let model = request.model.as_deref().unwrap_or("text-embedding-3-small");

        let openai_request = OpenAiEmbeddingRequest {
            model: model.to_string(),
            input: request.input,
            dimensions: request.dimensions,
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.config.base_url))
            .json(&openai_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(self.parse_error(status, &body));
        }

        let openai_response: OpenAiEmbeddingResponse = response.json().await?;

        Ok(crate::types::EmbeddingResponse {
            model: openai_response.model,
            embeddings: openai_response
                .data
                .into_iter()
                .map(|e| crate::types::Embedding {
                    index: e.index,
                    embedding: e.embedding,
                })
                .collect(),
            usage: crate::types::EmbeddingUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                total_tokens: openai_response.usage.total_tokens,
            },
        })
    }
}

impl OpenAiProvider {
    fn convert_choice_to_content(&self, choice: &OpenAiChoice) -> Vec<ContentBlock> {
        let mut content = Vec::new();

        // Add text content
        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlock::text(text));
            }
        }

        // Add tool calls
        if let Some(tool_calls) = &choice.message.tool_calls {
            for call in tool_calls {
                content.push(ContentBlock::ToolUse {
                    id: call.id.clone(),
                    name: call.function.name.clone(),
                    input: serde_json::from_str(&call.function.arguments)
                        .unwrap_or(serde_json::Value::Null),
                });
            }
        }

        content
    }
}

// OpenAI API types

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenAiResponseFormat>,
    stream: bool,
}

/// Response format specification for `OpenAI` API.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiResponseFormat {
    Text,
    JsonObject,
    JsonSchema { json_schema: OpenAiJsonSchema },
}

#[derive(Debug, Serialize)]
struct OpenAiJsonSchema {
    schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content can be a simple string or an array of content parts for multimodal messages.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OpenAiContent {
    /// Simple string content for text-only messages.
    Text(String),
    /// Array of content parts for multimodal messages (text + images).
    Parts(Vec<OpenAiContentPart>),
}

/// A content part in a multimodal message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiContentPart {
    /// Text content part.
    Text { text: String },
    /// Image URL content part (supports both URLs and base64 data URIs).
    ImageUrl { image_url: OpenAiImageUrl },
}

/// Image URL specification for `OpenAI` vision.
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiImageUrl {
    /// The image URL (can be a regular URL or a data URI like "data:image/jpeg;base64,...").
    url: String,
    /// Optional detail level: "auto", "low", or "high".
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl OpenAiMessage {
    fn from_message(msg: &Message) -> Self {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        // Check if message contains any images
        let has_images = msg
            .content
            .iter()
            .any(|c| matches!(c, crate::types::MessageContent::Image { .. }));

        let content = if has_images {
            // Use multimodal format with content parts
            let parts: Vec<OpenAiContentPart> = msg
                .content
                .iter()
                .filter_map(|c| match c {
                    crate::types::MessageContent::Text { text } => {
                        Some(OpenAiContentPart::Text { text: text.clone() })
                    }
                    crate::types::MessageContent::Image { source } => {
                        let url = match source {
                            crate::types::ImageSource::Base64 { media_type, data } => {
                                format!("data:{media_type};base64,{data}")
                            }
                            crate::types::ImageSource::Url { url } => url.clone(),
                        };
                        Some(OpenAiContentPart::ImageUrl {
                            image_url: OpenAiImageUrl { url, detail: None },
                        })
                    }
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(OpenAiContent::Parts(parts))
            }
        } else {
            // Use simple string format for text-only messages
            msg.text().map(|t| OpenAiContent::Text(t.to_string()))
        };

        Self {
            role: role.to_string(),
            content,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    id: String,
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

/// Tool call delta in streaming (different from non-streaming format)
#[derive(Debug, Deserialize)]
struct OpenAiStreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiError,
}

#[derive(Debug, Deserialize)]
struct OpenAiError {
    message: String,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}

// Embedding API types

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
    model: String,
    usage: OpenAiEmbeddingUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = OpenAiConfig::new("test-key")
            .base_url("https://custom.api.com")
            .organization("org-123")
            .default_model("gpt-3.5-turbo")
            .timeout(Duration::from_secs(60));

        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://custom.api.com");
        assert_eq!(config.organization, Some("org-123".to_string()));
        assert_eq!(config.default_model, "gpt-3.5-turbo");
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let openai_messages = OpenAiProvider::convert_messages(&messages);

        assert_eq!(openai_messages.len(), 3);
        assert_eq!(openai_messages[0].role, "system");
        assert_eq!(openai_messages[1].role, "user");
        assert_eq!(openai_messages[2].role, "assistant");
    }
}
