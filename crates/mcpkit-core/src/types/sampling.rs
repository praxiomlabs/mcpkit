//! Sampling types for MCP servers.
//!
//! Sampling allows servers to request LLM completions from the client.
//! This enables agentic workflows where servers can leverage
//! the client's AI capabilities.

use super::content::{
    AudioContent, ImageContent, Role, TextContent, ToolResultContent, ToolUseContent,
};
use super::meta::Meta;
use super::tool::Tool;
use serde::{Deserialize, Serialize};

/// A value that may be a single item or an array of items, matching the spec's
/// `T | T[]` unions (e.g. sampling message content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    /// A single item.
    One(T),
    /// An array of items.
    Many(Vec<T>),
}

/// A content block allowed in a sampling message (the spec's
/// `SamplingMessageContentBlock`): text/image/audio, plus tool-use loop blocks.
///
/// Note this is *not* the general [`Content`](super::content::Content)
/// (`ContentBlock`) — sampling excludes resource links/embeds and adds
/// `tool_use`/`tool_result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SamplingContent {
    /// Plain text.
    Text(TextContent),
    /// Image content.
    Image(ImageContent),
    /// Audio content.
    Audio(AudioContent),
    /// A tool call the model wants to make.
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseContent),
    /// The result of a tool call, fed back to the model.
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultContent),
}

impl SamplingContent {
    /// Create a text content block.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextContent {
            text: text.into(),
            annotations: None,
            meta: None,
        })
    }
}

/// How the model may use tools during sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    /// The model decides whether to use tools.
    Auto,
    /// The model must use at least one tool.
    Required,
    /// The model must not use tools.
    None,
}

/// Controls tool selection for a sampling request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolChoice {
    /// The tool-use mode; absent means the model decides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ToolChoiceMode>,
}

/// A message in a sampling conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    /// The role of the message sender.
    pub role: Role,
    /// The message content (one block or an array of blocks).
    pub content: OneOrMany<SamplingContent>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl SamplingMessage {
    /// Create a user message with text content.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: OneOrMany::One(SamplingContent::text(text)),
            meta: None,
        }
    }

    /// Create an assistant message with text content.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: OneOrMany::One(SamplingContent::text(text)),
            meta: None,
        }
    }

    /// Create a message with custom content.
    #[must_use]
    pub const fn with_content(role: Role, content: OneOrMany<SamplingContent>) -> Self {
        Self {
            role,
            content,
            meta: None,
        }
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
    #[serde(
        rename = "intelligencePriority",
        skip_serializing_if = "Option::is_none"
    )]
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
    /// Tools the model may call during sampling (2025-11-25).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Controls whether/how the model may call `tools`.
    #[serde(rename = "toolChoice", skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Request task-augmented execution (2025-11-25).
    ///
    /// When set, a client that declared
    /// `tasks.requests.sampling.createMessage` replies with a
    /// `CreateTaskResult` immediately and the `CreateMessageResult` is
    /// retrieved later via `tasks/result`. A client that did not declare it
    /// processes the request normally, ignoring this field (per spec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<super::task::TaskMetadata>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl CreateMessageRequest {
    /// Create a new sampling request.
    #[must_use]
    pub const fn new(messages: Vec<SamplingMessage>, max_tokens: u32) -> Self {
        Self {
            messages,
            max_tokens,
            model_preferences: None,
            system_prompt: None,
            include_context: None,
            temperature: None,
            stop_sequences: None,
            metadata: None,
            tools: None,
            tool_choice: None,
            task: None,
            meta: None,
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
    pub const fn include_context(mut self, context: IncludeContext) -> Self {
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

    /// Request task-augmented execution (2025-11-25).
    #[must_use]
    pub const fn with_task(mut self, task: super::task::TaskMetadata) -> Self {
        self.task = Some(task);
        self
    }
}

/// What context to include in sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum IncludeContext {
    /// Include no additional context.
    #[default]
    None,
    /// Include context from this server only.
    ThisServer,
    /// Include context from all connected servers.
    AllServers,
}

/// Result of a sampling request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageResult {
    /// The role of the response (always assistant).
    pub role: Role,
    /// The generated content (one block or an array — e.g. parallel tool calls).
    pub content: OneOrMany<SamplingContent>,
    /// The model used.
    pub model: String,
    /// Stop reason.
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl CreateMessageResult {
    /// Get the text if this is a single text-content response.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            OneOrMany::One(SamplingContent::Text(t)) => Some(&t.text),
            _ => None,
        }
    }
}

/// Reason why sampling stopped.
///
/// An **open** string in the spec: the known values are `endTurn`,
/// `stopSequence`, `maxTokens`, and `toolUse`, but implementations may report
/// others (preserved as [`Other`](StopReason::Other)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum StopReason {
    /// Hit the end of the response.
    EndTurn,
    /// Hit a stop sequence.
    StopSequence,
    /// Hit the max token limit.
    MaxTokens,
    /// The model stopped to call a tool.
    ToolUse,
    /// An implementation-specific stop reason.
    Other(String),
}

impl StopReason {
    /// The wire string for this stop reason.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "endTurn",
            Self::StopSequence => "stopSequence",
            Self::MaxTokens => "maxTokens",
            Self::ToolUse => "toolUse",
            Self::Other(s) => s,
        }
    }
}

impl From<String> for StopReason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "endTurn" => Self::EndTurn,
            "stopSequence" => Self::StopSequence,
            "maxTokens" => Self::MaxTokens,
            "toolUse" => Self::ToolUse,
            _ => Self::Other(s),
        }
    }
}

impl From<StopReason> for String {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::Other(s) => s,
            other => other.as_str().to_string(),
        }
    }
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Content;

    #[test]
    fn create_message_task_field_round_trips_and_omits() {
        // Omitted when unset (wire-compatible with pre-task requests).
        let req = CreateMessageRequest::simple("hi", 10);
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("task").is_none());

        // Round-trips when set.
        let req = req.with_task(crate::types::task::TaskMetadata { ttl: Some(60_000) });
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["task"], serde_json::json!({ "ttl": 60000 }));
        let back: CreateMessageRequest = serde_json::from_value(json).unwrap();
        assert_eq!(back.task.unwrap().ttl, Some(60_000));
    }

    #[test]
    fn test_sampling_message() {
        let user = SamplingMessage::user("Hello");
        assert!(matches!(user.role, Role::User));

        let assistant = SamplingMessage::assistant("Hi there!");
        assert!(matches!(assistant.role, Role::Assistant));
    }

    #[test]
    fn test_model_preferences() -> Result<(), Box<dyn std::error::Error>> {
        let prefs = ModelPreferences::smart().hint(ModelHint::name("claude-3-opus"));

        assert_eq!(prefs.intelligence_priority, Some(1.0));
        assert_eq!(prefs.hints.as_ref().ok_or("Expected hints")?.len(), 1);
        Ok(())
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
    fn test_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let request = CreateMessageRequest::simple("Hello", 100);
        let json = serde_json::to_string(&request)?;
        assert!(json.contains("\"maxTokens\":100"));
        Ok(())
    }

    #[test]
    fn stop_reason_uses_camelcase_and_preserves_unknown() {
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(StopReason::EndTurn).unwrap(),
            json!("endTurn")
        );
        assert_eq!(
            serde_json::to_value(StopReason::ToolUse).unwrap(),
            json!("toolUse")
        );
        // Unknown values round-trip through `Other`.
        let parsed: StopReason = serde_json::from_value(json!("guardrail")).unwrap();
        assert_eq!(parsed, StopReason::Other("guardrail".into()));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), json!("guardrail"));
        assert_eq!(
            serde_json::from_value::<StopReason>(json!("maxTokens")).unwrap(),
            StopReason::MaxTokens
        );
    }

    #[test]
    fn sampling_content_tool_blocks_round_trip() {
        use serde_json::json;

        let tool_use = SamplingContent::ToolUse(ToolUseContent {
            id: "call-1".into(),
            name: "search".into(),
            input: serde_json::from_value(json!({ "q": "rust" })).unwrap(),
            meta: None,
        });
        assert_eq!(
            serde_json::to_value(&tool_use).unwrap(),
            json!({ "type": "tool_use", "id": "call-1", "name": "search", "input": { "q": "rust" } })
        );

        let tool_result = SamplingContent::ToolResult(ToolResultContent {
            tool_use_id: "call-1".into(),
            content: vec![Content::text("42")],
            structured_content: None,
            is_error: None,
            meta: None,
        });
        let wire = serde_json::to_value(&tool_result).unwrap();
        assert_eq!(wire["type"], json!("tool_result"));
        assert_eq!(wire["toolUseId"], json!("call-1"));
        // Round-trips back to the same variant.
        let back: SamplingContent = serde_json::from_value(wire).unwrap();
        assert!(matches!(back, SamplingContent::ToolResult(_)));
    }

    #[test]
    fn one_or_many_serializes_single_and_array() {
        use serde_json::json;
        // Single block -> object; array -> array.
        let one = SamplingMessage::user("hi");
        assert_eq!(
            serde_json::to_value(&one.content).unwrap(),
            json!({ "type": "text", "text": "hi" })
        );
        let many: OneOrMany<SamplingContent> =
            OneOrMany::Many(vec![SamplingContent::text("a"), SamplingContent::text("b")]);
        assert!(serde_json::to_value(&many).unwrap().is_array());
        // Both forms parse back.
        assert!(matches!(
            serde_json::from_value::<OneOrMany<SamplingContent>>(
                json!({ "type": "text", "text": "x" })
            )
            .unwrap(),
            OneOrMany::One(_)
        ));
        assert!(matches!(
            serde_json::from_value::<OneOrMany<SamplingContent>>(
                json!([{ "type": "text", "text": "x" }])
            )
            .unwrap(),
            OneOrMany::Many(_)
        ));
    }

    #[test]
    fn include_context_serializes_camelcase() {
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(IncludeContext::None).unwrap(),
            json!("none")
        );
        assert_eq!(
            serde_json::to_value(IncludeContext::ThisServer).unwrap(),
            json!("thisServer")
        );
        assert_eq!(
            serde_json::to_value(IncludeContext::AllServers).unwrap(),
            json!("allServers")
        );
    }

    #[test]
    fn request_carries_tools_and_tool_choice() {
        let request = CreateMessageRequest {
            tools: Some(vec![]),
            tool_choice: Some(ToolChoice {
                mode: Some(ToolChoiceMode::Required),
            }),
            ..CreateMessageRequest::simple("hi", 10)
        };
        let wire = serde_json::to_value(&request).unwrap();
        assert!(wire.get("tools").is_some());
        assert_eq!(
            wire["toolChoice"],
            serde_json::json!({ "mode": "required" })
        );
    }
}
