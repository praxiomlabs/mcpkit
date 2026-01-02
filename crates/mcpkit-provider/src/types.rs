//! Core types for LLM provider interactions.
//!
//! This module defines the unified types used across all provider implementations,
//! enabling provider-agnostic code that works with any LLM.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The role of a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions for the model).
    System,
    /// User message.
    User,
    /// Assistant/model response.
    Assistant,
    /// Tool/function result.
    Tool,
}

impl Role {
    /// Check if this is a system role.
    #[must_use]
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System)
    }

    /// Check if this is a user role.
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }

    /// Check if this is an assistant role.
    #[must_use]
    pub const fn is_assistant(&self) -> bool {
        matches!(self, Self::Assistant)
    }
}

/// Content within a message, which can be text or other types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Plain text content.
    Text {
        /// The text content.
        text: String,
    },
    /// Image content (base64 or URL).
    Image {
        /// The image source (base64 data or URL).
        source: ImageSource,
    },
    /// Tool use request from the assistant.
    ToolUse {
        /// The tool call ID.
        id: String,
        /// The tool name.
        name: String,
        /// The tool input arguments.
        input: serde_json::Value,
    },
    /// Tool result from a previous tool use.
    ToolResult {
        /// The tool call ID this result corresponds to.
        tool_use_id: String,
        /// The result content.
        content: String,
        /// Whether this result represents an error.
        #[serde(default)]
        is_error: bool,
    },
}

impl MessageContent {
    /// Create text content.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Get the text content if this is a text block.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Source of an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data.
    Base64 {
        /// The media type (e.g., "image/png").
        media_type: String,
        /// The base64-encoded data.
        data: String,
    },
    /// URL to an image.
    Url {
        /// The image URL.
        url: String,
    },
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message sender.
    pub role: Role,
    /// The message content (can be multiple parts for multimodal).
    pub content: Vec<MessageContent>,
    /// Optional name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// Create a new message with a single text content.
    #[must_use]
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![MessageContent::text(text)],
            name: None,
        }
    }

    /// Create a system message.
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, text)
    }

    /// Create a user message.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, text)
    }

    /// Create an assistant message.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, text)
    }

    /// Create a tool result message.
    #[must_use]
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: vec![MessageContent::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error: false,
            }],
            name: None,
        }
    }

    /// Add a name to this message.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Get the text content of this message.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.content.first().and_then(MessageContent::as_text)
    }
}

/// Definition of a tool that can be called by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The name of the tool.
    pub name: String,
    /// A description of what the tool does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the input schema.
    #[must_use]
    pub fn input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = schema;
        self
    }
}

/// Legacy function definition (OpenAI-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// The name of the function.
    pub name: String,
    /// A description of what the function does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the function's parameters.
    pub parameters: serde_json::Value,
}

/// A tool call made by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// The unique ID of this tool call.
    pub id: String,
    /// The name of the tool being called.
    pub name: String,
    /// The arguments to pass to the tool (as JSON).
    pub arguments: serde_json::Value,
}

/// A legacy function call (OpenAI-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The name of the function.
    pub name: String,
    /// The arguments as a JSON string.
    pub arguments: String,
}

/// The result of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The ID of the tool call this result corresponds to.
    pub tool_call_id: String,
    /// The result content.
    pub content: String,
    /// Whether this result represents an error.
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful tool result.
    #[must_use]
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Create an error tool result.
    #[must_use]
    pub fn error(tool_call_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: message.into(),
            is_error: true,
        }
    }
}

/// How the model should choose which tool to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// Model decides whether to use tools.
    Auto,
    /// Model must use a tool.
    Required,
    /// Model should not use tools.
    None,
    /// Model must use this specific tool.
    Tool {
        /// The name of the required tool.
        name: String,
    },
}

impl Default for ToolChoice {
    fn default() -> Self {
        Self::Auto
    }
}

/// Response format specification for structured outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Text output (default).
    Text,
    /// JSON object output (model must be instructed to output JSON in the prompt).
    JsonObject,
    /// JSON output conforming to a schema.
    JsonSchema {
        /// The JSON schema the output should conform to.
        schema: serde_json::Value,
        /// Whether to strictly enforce the schema.
        #[serde(default)]
        strict: bool,
    },
}

impl Default for ResponseFormat {
    fn default() -> Self {
        Self::Text
    }
}

/// A request to complete a conversation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// The model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The conversation messages.
    pub messages: Vec<Message>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature for sampling (0.0 to 2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p (nucleus) sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Tools available for the model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// How the model should choose tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Response format for structured outputs (JSON mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// Additional provider-specific options.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl CompletionRequest {
    /// Create a new completion request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the messages.
    #[must_use]
    pub fn messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    /// Add a message.
    #[must_use]
    pub fn message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Set the maximum tokens.
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the temperature.
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the top-p value.
    #[must_use]
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Set the stop sequences.
    #[must_use]
    pub fn stop(mut self, stop: Vec<String>) -> Self {
        self.stop = Some(stop);
        self
    }

    /// Set the tools.
    #[must_use]
    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the tool choice.
    #[must_use]
    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }

    /// Enable streaming.
    #[must_use]
    pub fn stream(mut self) -> Self {
        self.stream = true;
        self
    }

    /// Set the response format (for JSON mode).
    #[must_use]
    pub fn response_format(mut self, format: ResponseFormat) -> Self {
        self.response_format = Some(format);
        self
    }

    /// Enable JSON object mode.
    #[must_use]
    pub fn json_mode(self) -> Self {
        self.response_format(ResponseFormat::JsonObject)
    }

    /// Enable JSON schema mode with a specific schema.
    #[must_use]
    pub fn json_schema(self, schema: serde_json::Value, strict: bool) -> Self {
        self.response_format(ResponseFormat::JsonSchema { schema, strict })
    }
}

/// A content block in a completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content.
    Text {
        /// The text.
        text: String,
    },
    /// A tool call.
    ToolUse {
        /// The tool call ID.
        id: String,
        /// The tool name.
        name: String,
        /// The tool input.
        input: serde_json::Value,
    },
}

impl ContentBlock {
    /// Create a text block.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Get the text if this is a text block.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    /// Get the tool use if this is a tool use block.
    #[must_use]
    pub fn as_tool_use(&self) -> Option<ToolCall> {
        match self {
            Self::ToolUse { id, name, input } => Some(ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: input.clone(),
            }),
            _ => None,
        }
    }
}

/// The reason the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// The model reached a natural stopping point.
    Stop,
    /// The model hit the max_tokens limit.
    Length,
    /// The model made a tool call.
    ToolUse,
    /// Content was filtered.
    ContentFilter,
    /// Generation is still in progress (streaming).
    #[serde(rename = "null")]
    Null,
}

impl FinishReason {
    /// Check if the model wants to use a tool.
    #[must_use]
    pub const fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse)
    }

    /// Check if the model stopped naturally.
    #[must_use]
    pub const fn is_stop(&self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// Token usage information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens generated.
    pub completion_tokens: u32,
    /// Total tokens (prompt + completion).
    pub total_tokens: u32,
    /// Cached tokens (if supported by provider).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

impl Usage {
    /// Create empty usage.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_tokens: None,
        }
    }

    /// Create usage with specific values.
    #[must_use]
    pub const fn with_tokens(prompt: u32, completion: u32) -> Self {
        Self {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            cached_tokens: None,
        }
    }
}

/// A completion response from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Unique ID for this completion.
    pub id: String,
    /// The model that generated this response.
    pub model: String,
    /// The content blocks in the response.
    pub content: Vec<ContentBlock>,
    /// Why the model stopped generating.
    pub finish_reason: FinishReason,
    /// Token usage.
    pub usage: Usage,
}

impl CompletionResponse {
    /// Get the text content of the response.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        let texts: Vec<&str> = self
            .content
            .iter()
            .filter_map(ContentBlock::as_text)
            .collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join(""))
        }
    }

    /// Get all tool calls from the response.
    #[must_use]
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(ContentBlock::as_tool_use)
            .collect()
    }

    /// Check if the response contains tool calls.
    #[must_use]
    pub fn has_tool_calls(&self) -> bool {
        self.content
            .iter()
            .any(|c| matches!(c, ContentBlock::ToolUse { .. }))
    }
}

/// An event in a streaming completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// The stream has started.
    Start {
        /// The completion ID.
        id: String,
        /// The model being used.
        model: String,
    },
    /// A content delta (text or tool).
    ContentDelta {
        /// The index of the content block being updated.
        index: usize,
        /// The delta content.
        delta: ContentDelta,
    },
    /// A tool use block has started.
    ToolUseStart {
        /// The index of this content block.
        index: usize,
        /// The tool call ID.
        id: String,
        /// The tool name.
        name: String,
    },
    /// The stream has ended.
    Stop {
        /// The finish reason.
        finish_reason: FinishReason,
        /// Token usage.
        usage: Usage,
    },
    /// An error occurred.
    Error {
        /// The error message.
        message: String,
    },
}

/// A delta in streaming content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    /// Text delta.
    Text {
        /// The text fragment.
        text: String,
    },
    /// Tool input delta (JSON fragment).
    ToolInput {
        /// The partial JSON input.
        partial_json: String,
    },
}

/// Information about a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model ID.
    pub id: String,
    /// Human-readable name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Maximum context length in tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    /// Maximum output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Whether the model supports tools.
    #[serde(default)]
    pub supports_tools: bool,
    /// Whether the model supports vision.
    #[serde(default)]
    pub supports_vision: bool,
    /// Cost per 1M input tokens (in USD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_cost_per_million: Option<f64>,
    /// Cost per 1M output tokens (in USD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_cost_per_million: Option<f64>,
}

impl ModelInfo {
    /// Create a new model info.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            context_length: None,
            max_output_tokens: None,
            supports_tools: false,
            supports_vision: false,
            input_cost_per_million: None,
            output_cost_per_million: None,
        }
    }

    /// Estimate the cost for given token usage.
    #[must_use]
    pub fn estimate_cost(&self, usage: &Usage) -> Option<f64> {
        let input_cost = self.input_cost_per_million? * f64::from(usage.prompt_tokens) / 1_000_000.0;
        let output_cost =
            self.output_cost_per_million? * f64::from(usage.completion_tokens) / 1_000_000.0;
        Some(input_cost + output_cost)
    }
}

/// Request for generating embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// The text(s) to embed.
    pub input: Vec<String>,
    /// The model to use for embeddings (optional, uses provider default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Dimensions of the embedding vector (optional, model-dependent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

impl EmbeddingRequest {
    /// Create a new embedding request with a single input.
    #[must_use]
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: vec![input.into()],
            model: None,
            dimensions: None,
        }
    }

    /// Create an embedding request with multiple inputs.
    #[must_use]
    pub fn batch(inputs: Vec<String>) -> Self {
        Self {
            input: inputs,
            model: None,
            dimensions: None,
        }
    }

    /// Set the model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the desired embedding dimensions.
    #[must_use]
    pub fn dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }
}

/// Response containing embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The model used for embedding.
    pub model: String,
    /// The embedding vectors, one per input.
    pub embeddings: Vec<Embedding>,
    /// Token usage for the request.
    pub usage: EmbeddingUsage,
}

/// A single embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// Index of this embedding in the request's input array.
    pub index: usize,
    /// The embedding vector.
    pub embedding: Vec<f32>,
}

impl EmbeddingResponse {
    /// Calculate cosine similarity between two embeddings by index.
    ///
    /// Returns a value between -1.0 and 1.0, where:
    /// - 1.0 means the vectors are identical in direction
    /// - 0.0 means the vectors are orthogonal (unrelated)
    /// - -1.0 means the vectors point in opposite directions
    ///
    /// # Errors
    ///
    /// Returns `None` if either index is out of bounds or if the embeddings
    /// have different dimensions.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let response = provider.embed(EmbeddingRequest::batch(vec![
    ///     "Hello world".to_string(),
    ///     "Hi there".to_string(),
    /// ])).await?;
    ///
    /// let similarity = response.cosine_similarity(0, 1).unwrap();
    /// println!("Similarity: {:.4}", similarity);
    /// ```
    #[must_use]
    pub fn cosine_similarity(&self, index_a: usize, index_b: usize) -> Option<f32> {
        let a = self.embeddings.get(index_a)?;
        let b = self.embeddings.get(index_b)?;

        if a.embedding.len() != b.embedding.len() {
            return None;
        }

        let dot_product: f32 = a
            .embedding
            .iter()
            .zip(b.embedding.iter())
            .map(|(x, y)| x * y)
            .sum();

        let magnitude_a: f32 = a.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return None;
        }

        Some(dot_product / (magnitude_a * magnitude_b))
    }

    /// Calculate dot product between two embeddings by index.
    ///
    /// For normalized embeddings (like OpenAI's), dot product equals cosine similarity
    /// and is faster to compute.
    ///
    /// # Errors
    ///
    /// Returns `None` if either index is out of bounds or if the embeddings
    /// have different dimensions.
    #[must_use]
    pub fn dot_product(&self, index_a: usize, index_b: usize) -> Option<f32> {
        let a = self.embeddings.get(index_a)?;
        let b = self.embeddings.get(index_b)?;

        if a.embedding.len() != b.embedding.len() {
            return None;
        }

        Some(
            a.embedding
                .iter()
                .zip(b.embedding.iter())
                .map(|(x, y)| x * y)
                .sum(),
        )
    }

    /// Calculate Euclidean distance between two embeddings by index.
    ///
    /// Returns the L2 distance. Smaller values mean more similar.
    ///
    /// # Errors
    ///
    /// Returns `None` if either index is out of bounds or if the embeddings
    /// have different dimensions.
    #[must_use]
    pub fn euclidean_distance(&self, index_a: usize, index_b: usize) -> Option<f32> {
        let a = self.embeddings.get(index_a)?;
        let b = self.embeddings.get(index_b)?;

        if a.embedding.len() != b.embedding.len() {
            return None;
        }

        let sum_squared_diff: f32 = a
            .embedding
            .iter()
            .zip(b.embedding.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum();

        Some(sum_squared_diff.sqrt())
    }
}

impl Embedding {
    /// Get the dimensionality of this embedding.
    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.embedding.len()
    }

    /// Compute cosine similarity with another embedding.
    ///
    /// Returns `None` if dimensions don't match or either vector has zero magnitude.
    #[must_use]
    pub fn cosine_similarity(&self, other: &Embedding) -> Option<f32> {
        if self.embedding.len() != other.embedding.len() {
            return None;
        }

        let dot_product: f32 = self
            .embedding
            .iter()
            .zip(other.embedding.iter())
            .map(|(x, y)| x * y)
            .sum();

        let magnitude_a: f32 = self.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = other.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return None;
        }

        Some(dot_product / (magnitude_a * magnitude_b))
    }
}

/// Token usage for an embedding request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    /// Number of tokens in the input.
    pub prompt_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text(), Some("Hello"));

        let msg = Message::system("You are helpful.");
        assert_eq!(msg.role, Role::System);
    }

    #[test]
    fn test_completion_request_builder() {
        let request = CompletionRequest::new()
            .model("gpt-4")
            .message(Message::user("Hello"))
            .max_tokens(100)
            .temperature(0.7);

        assert_eq!(request.model, Some("gpt-4".to_string()));
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.max_tokens, Some(100));
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_completion_response_text() {
        let response = CompletionResponse {
            id: "test".to_string(),
            model: "gpt-4".to_string(),
            content: vec![
                ContentBlock::text("Hello "),
                ContentBlock::text("world!"),
            ],
            finish_reason: FinishReason::Stop,
            usage: Usage::with_tokens(10, 5),
        };

        assert_eq!(response.text(), Some("Hello world!".to_string()));
    }

    #[test]
    fn test_model_cost_estimation() {
        let model = ModelInfo {
            id: "gpt-4".to_string(),
            name: Some("GPT-4".to_string()),
            context_length: Some(8192),
            max_output_tokens: Some(4096),
            supports_tools: true,
            supports_vision: false,
            input_cost_per_million: Some(30.0),
            output_cost_per_million: Some(60.0),
        };

        let usage = Usage::with_tokens(1000, 500);
        let cost = model.estimate_cost(&usage).unwrap();
        assert!((cost - 0.06).abs() < 0.0001); // $0.03 input + $0.03 output
    }

    #[test]
    fn test_embedding_cosine_similarity() {
        let response = EmbeddingResponse {
            model: "test-model".to_string(),
            embeddings: vec![
                Embedding {
                    index: 0,
                    embedding: vec![1.0, 0.0, 0.0],
                },
                Embedding {
                    index: 1,
                    embedding: vec![1.0, 0.0, 0.0],
                },
                Embedding {
                    index: 2,
                    embedding: vec![0.0, 1.0, 0.0],
                },
            ],
            usage: EmbeddingUsage::default(),
        };

        // Identical vectors should have similarity 1.0
        let sim = response.cosine_similarity(0, 1).unwrap();
        assert!((sim - 1.0).abs() < 0.0001);

        // Orthogonal vectors should have similarity 0.0
        let sim = response.cosine_similarity(0, 2).unwrap();
        assert!(sim.abs() < 0.0001);

        // Out of bounds should return None
        assert!(response.cosine_similarity(0, 10).is_none());
    }

    #[test]
    fn test_embedding_dot_product() {
        let response = EmbeddingResponse {
            model: "test-model".to_string(),
            embeddings: vec![
                Embedding {
                    index: 0,
                    embedding: vec![1.0, 2.0, 3.0],
                },
                Embedding {
                    index: 1,
                    embedding: vec![4.0, 5.0, 6.0],
                },
            ],
            usage: EmbeddingUsage::default(),
        };

        // Dot product: 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
        let dot = response.dot_product(0, 1).unwrap();
        assert!((dot - 32.0).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_euclidean_distance() {
        let response = EmbeddingResponse {
            model: "test-model".to_string(),
            embeddings: vec![
                Embedding {
                    index: 0,
                    embedding: vec![0.0, 0.0, 0.0],
                },
                Embedding {
                    index: 1,
                    embedding: vec![3.0, 4.0, 0.0],
                },
            ],
            usage: EmbeddingUsage::default(),
        };

        // Distance: sqrt(3^2 + 4^2) = 5
        let dist = response.euclidean_distance(0, 1).unwrap();
        assert!((dist - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_single_embedding_similarity() {
        let a = Embedding {
            index: 0,
            embedding: vec![1.0, 0.0],
        };
        let b = Embedding {
            index: 1,
            embedding: vec![0.707, 0.707], // ~45 degrees
        };

        let sim = a.cosine_similarity(&b).unwrap();
        // cos(45°) ≈ 0.707
        assert!((sim - 0.707).abs() < 0.01);

        // Dimensions
        assert_eq!(a.dimensions(), 2);
    }
}
