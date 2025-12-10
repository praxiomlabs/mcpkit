//! Sampling types for MCP servers.
//!
//! Sampling allows servers to request LLM completions from the client.
//! This enables powerful agentic workflows where servers can leverage
//! the client's AI capabilities.

use super::content::{Content, Role};
use serde::{Deserialize, Serialize};

/// A message in a sampling conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    /// The role of the message sender.
    pub role: Role,
    /// The message content.
    pub content: Content,
}

impl SamplingMessage {
    /// Create a user message with text content.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::text(text),
        }
    }

    /// Create an assistant message with text content.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::text(text),
        }
    }

    /// Create a message with custom content.
    #[must_use]
    pub fn with_content(role: Role, content: Content) -> Self {
        Self { role, content }
    }
}

/// Model preferences for sampling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPreferences {
    /// Hints for model selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,
    /// Priority for cost optimization (0.0 = prioritize cost, 1.0 = ignore cost).
    #[serde(rename = "costPriority", skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
    /// Priority for speed (0.0 = prioritize speed, 1.0 = ignore speed).
    #[serde(rename = "speedPriority", skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,
    /// Priority for intelligence (0.0 = basic model, 1.0 = most capable).
    #[serde(rename = "intelligencePriority", skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,
}

impl ModelPreferences {
    /// Create preferences optimized for speed.
    #[must_use]
    pub fn fast() -> Self {
        Self {
            speed_priority: Some(0.0),
            ..Default::default()
        }
    }

    /// Create preferences optimized for quality.
    #[must_use]
    pub fn smart() -> Self {
        Self {
            intelligence_priority: Some(1.0),
            ..Default::default()
        }
    }

    /// Create preferences optimized for cost.
    #[must_use]
    pub fn cheap() -> Self {
        Self {
            cost_priority: Some(0.0),
            ..Default::default()
        }
    }

    /// Add a model hint.
    #[must_use]
    pub fn hint(mut self, hint: ModelHint) -> Self {
        self.hints.get_or_insert_with(Vec::new).push(hint);
        self
    }
}

/// A hint for model selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Suggested model name or pattern.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ModelHint {
    /// Create a hint for a specific model name.
    #[must_use]
    pub fn name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
        }
    }
}

/// Request for creating a sampling message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequest {
    /// The conversation messages.
    pub messages: Vec<SamplingMessage>,
    /// Maximum tokens to generate.
    #[serde(rename = "maxTokens")]
    pub max_tokens: u32,
    /// Model preferences.
    #[serde(rename = "modelPreferences", skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
    /// System prompt.
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Include context from the MCP server.
    #[serde(rename = "includeContext", skip_serializing_if = "Option::is_none")]
    pub include_context: Option<IncludeContext>,
    /// Temperature for sampling (0.0 to 2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Stop sequences.
    #[serde(rename = "stopSequences", skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl CreateMessageRequest {
    /// Create a new sampling request.
    #[must_use]
    pub fn new(messages: Vec<SamplingMessage>, max_tokens: u32) -> Self {
        Self {
            messages,
            max_tokens,
            model_preferences: None,
            system_prompt: None,
            include_context: None,
            temperature: None,
            stop_sequences: None,
            metadata: None,
        }
    }

    /// Create a simple request with a single user message.
    #[must_use]
    pub fn simple(prompt: impl Into<String>, max_tokens: u32) -> Self {
        Self::new(vec![SamplingMessage::user(prompt)], max_tokens)
    }

    /// Set model preferences.
    #[must_use]
    pub fn model_preferences(mut self, prefs: ModelPreferences) -> Self {
        self.model_preferences = Some(prefs);
        self
    }

    /// Set the system prompt.
    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set context inclusion.
    #[must_use]
    pub fn include_context(mut self, context: IncludeContext) -> Self {
        self.include_context = Some(context);
        self
    }

    /// Set the temperature.
    #[must_use]
    pub fn temperature(mut self, temp: f64) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }

    /// Add a stop sequence.
    #[must_use]
    pub fn stop_sequence(mut self, seq: impl Into<String>) -> Self {
        self.stop_sequences
            .get_or_insert_with(Vec::new)
            .push(seq.into());
        self
    }
}

/// What context to include in sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncludeContext {
    /// Include no additional context.
    None,
    /// Include context from this server only.
    ThisServer,
    /// Include context from all connected servers.
    AllServers,
}

impl Default for IncludeContext {
    fn default() -> Self {
        Self::None
    }
}

/// Result of a sampling request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageResult {
    /// The role of the response (always assistant).
    pub role: Role,
    /// The generated content.
    pub content: Content,
    /// The model used.
    pub model: String,
    /// Stop reason.
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
}

impl CreateMessageResult {
    /// Get the text content if this is a text response.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        self.content.as_text()
    }
}

/// Reason why sampling stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Hit the end of the response.
    EndTurn,
    /// Hit a stop sequence.
    StopSequence,
    /// Hit the max token limit.
    MaxTokens,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EndTurn => write!(f, "end_turn"),
            Self::StopSequence => write!(f, "stop_sequence"),
            Self::MaxTokens => write!(f, "max_tokens"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_message() {
        let user = SamplingMessage::user("Hello");
        assert!(matches!(user.role, Role::User));

        let assistant = SamplingMessage::assistant("Hi there!");
        assert!(matches!(assistant.role, Role::Assistant));
    }

    #[test]
    fn test_model_preferences() {
        let prefs = ModelPreferences::smart()
            .hint(ModelHint::name("claude-3-opus"));

        assert_eq!(prefs.intelligence_priority, Some(1.0));
        assert_eq!(prefs.hints.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_create_message_request() {
        let request = CreateMessageRequest::simple("What is 2+2?", 100)
            .system_prompt("You are a helpful assistant")
            .temperature(0.7)
            .model_preferences(ModelPreferences::fast());

        assert_eq!(request.max_tokens, 100);
        assert_eq!(request.temperature, Some(0.7));
        assert!(request.system_prompt.is_some());
    }

    #[test]
    fn test_temperature_clamping() {
        let request = CreateMessageRequest::simple("Test", 100).temperature(3.0);
        assert_eq!(request.temperature, Some(2.0));

        let request = CreateMessageRequest::simple("Test", 100).temperature(-1.0);
        assert_eq!(request.temperature, Some(0.0));
    }

    #[test]
    fn test_serialization() {
        let request = CreateMessageRequest::simple("Hello", 100);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"maxTokens\":100"));
    }
}
