//! Ollama provider implementation.
//!
//! This module provides an implementation of the [`Provider`] trait for Ollama,
//! enabling local LLM inference with models like Llama, Mistral, and others.
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_provider::ollama::OllamaProvider;
//! use mcpkit_provider::{Provider, CompletionRequest, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OllamaProvider::new()?;
//!
//!     let request = CompletionRequest::new()
//!         .model("llama3.2")
//!         .message(Message::user("Hello!"));
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
    CompletionRequest, CompletionResponse, ContentBlock, FinishReason, Message, ModelInfo, Role,
    StreamEvent, ToolDefinition, Usage,
};

/// Default Ollama API base URL.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Default model to use if none specified.
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Configuration for the Ollama provider.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL for the Ollama API.
    pub base_url: String,
    /// Default model to use.
    pub default_model: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(300), // Longer timeout for local inference
        }
    }
}

impl OllamaConfig {
    /// Create a new config with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
}

/// Ollama provider implementation.
pub struct OllamaProvider {
    config: OllamaConfig,
    client: reqwest::Client,
    info: ProviderInfo,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new() -> ProviderResult<Self> {
        Self::with_config(OllamaConfig::default())
    }

    /// Create a new Ollama provider with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn with_config(config: OllamaConfig) -> ProviderResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .map_err(|e| ProviderError::Configuration {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let info = ProviderInfo::new("ollama", "Ollama")
            .capabilities(ProviderCapabilities {
                streaming: true,
                tools: true, // Ollama supports tools with compatible models
                vision: true, // Some models support vision
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

    /// Convert our messages to Ollama's format.
    fn convert_messages(messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                // Extract base64 images from message content
                let images: Vec<String> = msg
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        crate::types::MessageContent::Image { source } => match source {
                            // Ollama only supports base64 images
                            crate::types::ImageSource::Base64 { data, .. } => Some(data.clone()),
                            // URL images are not supported by Ollama API
                            crate::types::ImageSource::Url { .. } => None,
                        },
                        _ => None,
                    })
                    .collect();

                OllamaMessage {
                    role: role.to_string(),
                    content: msg.text().unwrap_or("").to_string(),
                    images: if images.is_empty() {
                        None
                    } else {
                        Some(images)
                    },
                    tool_calls: None,
                }
            })
            .collect()
    }

    /// Convert our tool definitions to Ollama's format.
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                tool_type: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone().unwrap_or_default(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    }

    /// Convert our response format to Ollama's format.
    fn convert_response_format(
        format: &Option<crate::types::ResponseFormat>,
    ) -> Option<OllamaFormat> {
        format.as_ref().and_then(|f| match f {
            // Ollama doesn't have a "text" format; omit the field entirely
            crate::types::ResponseFormat::Text => None,
            crate::types::ResponseFormat::JsonObject => {
                Some(OllamaFormat::Json("json".to_string()))
            }
            crate::types::ResponseFormat::JsonSchema { schema, .. } => {
                Some(OllamaFormat::Schema(schema.clone()))
            }
        })
    }

    /// Parse an Ollama error response.
    fn parse_error(&self, status: reqwest::StatusCode, body: &str) -> ProviderError {
        if let Ok(error) = serde_json::from_str::<OllamaErrorResponse>(body) {
            return ProviderError::Other {
                provider: "ollama".to_string(),
                message: error.error,
                code: None,
            };
        }

        ProviderError::UnexpectedResponse {
            provider: "ollama".to_string(),
            message: format!("HTTP {status}: {body}"),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    #[instrument(skip(self, request), fields(model = %self.effective_model(&request)))]
    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let model = self.effective_model(&request).to_string();

        let ollama_request = OllamaChatRequest {
            model: model.clone(),
            messages: Self::convert_messages(&request.messages),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            stream: false,
            options: Some(OllamaOptions {
                temperature: request.temperature,
                top_p: request.top_p,
                num_predict: request.max_tokens.map(|t| t as i32),
                stop: request.stop.clone(),
            }),
            format: Self::convert_response_format(&request.response_format),
        };

        debug!("Sending request to Ollama");

        let response = self
            .client
            .post(format!("{}/api/chat", self.config.base_url))
            .json(&ollama_request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(self.parse_error(status, &body));
        }

        let ollama_response: OllamaChatResponse =
            serde_json::from_str(&body).map_err(|e| ProviderError::UnexpectedResponse {
                provider: "ollama".to_string(),
                message: format!("Failed to parse response: {e}"),
            })?;

        let content = vec![ContentBlock::text(ollama_response.message.content)];
        let finish_reason = if ollama_response.done {
            FinishReason::Stop
        } else {
            FinishReason::Length
        };

        // Ollama doesn't provide detailed token counts in non-streaming mode
        let usage = Usage {
            prompt_tokens: ollama_response.prompt_eval_count.unwrap_or(0),
            completion_tokens: ollama_response.eval_count.unwrap_or(0),
            total_tokens: ollama_response.prompt_eval_count.unwrap_or(0)
                + ollama_response.eval_count.unwrap_or(0),
            cached_tokens: None,
        };

        Ok(CompletionResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4()),
            model: ollama_response.model,
            content,
            finish_reason,
            usage,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> ProviderResult<CompletionStream> {
        let model = self.effective_model(&request).to_string();

        let ollama_request = OllamaChatRequest {
            model: model.clone(),
            messages: Self::convert_messages(&request.messages),
            tools: request.tools.as_ref().map(|t| Self::convert_tools(t)),
            stream: true,
            options: Some(OllamaOptions {
                temperature: request.temperature,
                top_p: request.top_p,
                num_predict: request.max_tokens.map(|t| t as i32),
                stop: request.stop.clone(),
            }),
            format: Self::convert_response_format(&request.response_format),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.config.base_url))
            .json(&ollama_request)
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
            let mut tool_call_index: usize = 1; // Start at 1, text is at 0

            tokio::pin!(bytes_stream);

            while let Some(chunk) = bytes_stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Ollama sends newline-delimited JSON
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            match serde_json::from_str::<OllamaStreamChunk>(&line) {
                                Ok(chunk) => {
                                    if !emitted_start {
                                        yield Ok(StreamEvent::Start {
                                            id: format!("ollama-{}", uuid::Uuid::new_v4()),
                                            model: model_clone.clone(),
                                        });
                                        emitted_start = true;
                                    }

                                    // Handle text content
                                    if !chunk.message.content.is_empty() {
                                        yield Ok(StreamEvent::ContentDelta {
                                            index: 0,
                                            delta: crate::types::ContentDelta::Text {
                                                text: chunk.message.content,
                                            },
                                        });
                                    }

                                    // Handle tool calls (Ollama sends them complete in the message)
                                    if let Some(tool_calls) = &chunk.message.tool_calls {
                                        for tool_call in tool_calls {
                                            // Emit ToolUseStart
                                            let tool_id = format!("ollama-tool-{}", uuid::Uuid::new_v4());
                                            yield Ok(StreamEvent::ToolUseStart {
                                                index: tool_call_index,
                                                id: tool_id,
                                                name: tool_call.function.name.clone(),
                                            });

                                            // Emit the complete JSON as a single delta
                                            let json_str = serde_json::to_string(&tool_call.function.arguments)
                                                .unwrap_or_else(|_| "{}".to_string());
                                            yield Ok(StreamEvent::ContentDelta {
                                                index: tool_call_index,
                                                delta: crate::types::ContentDelta::ToolInput {
                                                    partial_json: json_str,
                                                },
                                            });

                                            tool_call_index += 1;
                                        }
                                    }

                                    if chunk.done {
                                        usage.prompt_tokens = chunk.prompt_eval_count.unwrap_or(0);
                                        usage.completion_tokens = chunk.eval_count.unwrap_or(0);
                                        usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;

                                        let finish_reason = if tool_call_index > 1 {
                                            FinishReason::ToolUse
                                        } else {
                                            FinishReason::Stop
                                        };

                                        yield Ok(StreamEvent::Stop {
                                            finish_reason,
                                            usage: usage.clone(),
                                        });
                                    }
                                }
                                Err(e) => {
                                    yield Ok(StreamEvent::Error {
                                        message: format!("Failed to parse Ollama chunk: {}", e),
                                    });
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
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(self.parse_error(status, &body));
        }

        let models_response: OllamaTagsResponse =
            serde_json::from_str(&body).map_err(|e| ProviderError::UnexpectedResponse {
                provider: "ollama".to_string(),
                message: format!("Failed to parse models response: {e}"),
            })?;

        Ok(models_response
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name.clone(),
                name: Some(m.name),
                context_length: None, // Ollama doesn't provide this in tags
                max_output_tokens: None,
                supports_tools: true, // Assume true, depends on model
                supports_vision: m.details.families.iter().any(|f| f.contains("vision")),
                input_cost_per_million: Some(0.0), // Local = free
                output_cost_per_million: Some(0.0),
            })
            .collect())
    }

    async fn get_model(&self, model_id: &str) -> ProviderResult<ModelInfo> {
        // Try to get model info from Ollama's show endpoint
        let response = self
            .client
            .post(format!("{}/api/show", self.config.base_url))
            .json(&serde_json::json!({ "name": model_id }))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            // Model might not be pulled yet
            return Ok(ModelInfo::new(model_id));
        }

        let body = response.text().await?;
        let show_response: OllamaShowResponse =
            serde_json::from_str(&body).unwrap_or(OllamaShowResponse::default());

        Ok(ModelInfo {
            id: model_id.to_string(),
            name: Some(model_id.to_string()),
            context_length: show_response
                .model_info
                .and_then(|i| i.get("context_length").and_then(|v| v.as_u64()))
                .map(|v| v as u32),
            max_output_tokens: None,
            supports_tools: true,
            supports_vision: show_response
                .details
                .map(|d| d.families.iter().any(|f| f.contains("vision")))
                .unwrap_or(false),
            input_cost_per_million: Some(0.0),
            output_cost_per_million: Some(0.0),
        })
    }

    #[instrument(skip(self, request), fields(model = %request.model.as_deref().unwrap_or("nomic-embed-text")))]
    async fn embed(
        &self,
        request: crate::types::EmbeddingRequest,
    ) -> ProviderResult<crate::types::EmbeddingResponse> {
        let model = request
            .model
            .as_deref()
            .unwrap_or("nomic-embed-text")
            .to_string();

        let ollama_request = OllamaEmbedRequest {
            model: model.clone(),
            input: request.input,
        };

        let response = self
            .client
            .post(format!("{}/api/embed", self.config.base_url))
            .json(&ollama_request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(self.parse_error(status, &body));
        }

        let ollama_response: OllamaEmbedResponse = response.json().await?;

        // Convert to our format - Ollama doesn't provide token usage info
        let embeddings = ollama_response
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| crate::types::Embedding { index, embedding })
            .collect();

        Ok(crate::types::EmbeddingResponse {
            model: ollama_response.model,
            embeddings,
            usage: crate::types::EmbeddingUsage::default(),
        })
    }
}

// Ollama API types

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// Format specification: "json" for JSON mode, or a JSON schema for structured output.
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<OllamaFormat>,
}

/// Format specification for Ollama JSON/structured output.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OllamaFormat {
    /// Simple JSON mode ("json").
    Json(String),
    /// JSON schema for structured output.
    Schema(serde_json::Value),
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    /// Base64-encoded images for vision models.
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    message: OllamaMessage,
    done: bool,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelTag>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelTag {
    name: String,
    details: OllamaModelDetails,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaModelDetails {
    #[serde(default)]
    families: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaShowResponse {
    details: Option<OllamaModelDetails>,
    model_info: Option<serde_json::Map<String, serde_json::Value>>,
}

// Embedding API types

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    model: String,
    embeddings: Vec<Vec<f32>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = OllamaConfig::new()
            .base_url("http://custom:11434")
            .default_model("mistral")
            .timeout(Duration::from_secs(600));

        assert_eq!(config.base_url, "http://custom:11434");
        assert_eq!(config.default_model, "mistral");
        assert_eq!(config.timeout, Duration::from_secs(600));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let ollama_messages = OllamaProvider::convert_messages(&messages);

        assert_eq!(ollama_messages.len(), 3);
        assert_eq!(ollama_messages[0].role, "system");
        assert_eq!(ollama_messages[1].role, "user");
        assert_eq!(ollama_messages[2].role, "assistant");
    }
}
