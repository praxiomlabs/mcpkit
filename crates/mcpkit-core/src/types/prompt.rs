//! Prompt types for MCP servers.
//!
//! Prompts are templated messages that servers can provide to AI assistants.
//! They allow servers to define reusable message patterns with arguments.

use super::content::{Content, Role};
use serde::{Deserialize, Serialize};

/// A prompt definition exposed by an MCP server.
///
/// Prompts are templates for messages that can be parameterized with
/// arguments. They help standardize common interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    /// Unique name of the prompt.
    pub name: String,
    /// Human-readable description of what the prompt does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Arguments that the prompt accepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

impl Prompt {
    /// Create a new prompt with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            arguments: None,
        }
    }

    /// Set the prompt description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to the prompt.
    #[must_use]
    pub fn argument(mut self, arg: PromptArgument) -> Self {
        self.arguments.get_or_insert_with(Vec::new).push(arg);
        self
    }

    /// Add a required string argument.
    #[must_use]
    pub fn required_arg(self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.argument(PromptArgument::required(name, description))
    }

    /// Add an optional string argument.
    #[must_use]
    pub fn optional_arg(self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.argument(PromptArgument::optional(name, description))
    }
}

/// An argument that a prompt accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    /// Name of the argument.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this argument is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

impl PromptArgument {
    /// Create a required argument.
    #[must_use]
    pub fn required(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            required: Some(true),
        }
    }

    /// Create an optional argument.
    #[must_use]
    pub fn optional(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            required: Some(false),
        }
    }
}

/// A message in a prompt result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    /// The role of the message sender.
    pub role: Role,
    /// The message content.
    pub content: Content,
}

impl PromptMessage {
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

/// The result of getting a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptResult {
    /// Optional description of the rendered prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The prompt messages.
    pub messages: Vec<PromptMessage>,
}

impl GetPromptResult {
    /// Create a prompt result with a single user message.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            description: None,
            messages: vec![PromptMessage::user(text)],
        }
    }

    /// Create a prompt result with a single assistant message.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            description: None,
            messages: vec![PromptMessage::assistant(text)],
        }
    }

    /// Create a prompt result with multiple messages.
    #[must_use]
    pub fn messages(messages: Vec<PromptMessage>) -> Self {
        Self {
            description: None,
            messages,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Simplified output type for prompt handlers.
#[derive(Debug, Clone)]
pub enum PromptOutput {
    /// A single user message.
    User(String),
    /// A single assistant message.
    Assistant(String),
    /// Multiple messages.
    Messages(Vec<PromptMessage>),
    /// Full result with description.
    Full(GetPromptResult),
}

impl PromptOutput {
    /// Create a user message.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self::User(text.into())
    }

    /// Create an assistant message.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::Assistant(text.into())
    }

    /// Create a conversation with multiple messages.
    #[must_use]
    pub fn conversation(messages: Vec<PromptMessage>) -> Self {
        Self::Messages(messages)
    }
}

impl From<PromptOutput> for GetPromptResult {
    fn from(output: PromptOutput) -> Self {
        match output {
            PromptOutput::User(text) => GetPromptResult::user(text),
            PromptOutput::Assistant(text) => GetPromptResult::assistant(text),
            PromptOutput::Messages(messages) => GetPromptResult::messages(messages),
            PromptOutput::Full(result) => result,
        }
    }
}

/// Request parameters for listing prompts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListPromptsRequest {
    /// Cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for listing prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPromptsResult {
    /// The list of available prompts.
    pub prompts: Vec<Prompt>,
    /// Cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Request parameters for getting a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptRequest {
    /// Name of the prompt to get.
    pub name: String,
    /// Arguments to pass to the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Notification that the prompt list has changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptListChangedNotification {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_builder() {
        let prompt = Prompt::new("summarize")
            .description("Summarize a document")
            .required_arg("document", "The document to summarize")
            .optional_arg("length", "Target length (short, medium, long)");

        assert_eq!(prompt.name, "summarize");
        assert_eq!(prompt.arguments.as_ref().unwrap().len(), 2);
        assert!(prompt.arguments.as_ref().unwrap()[0].required.unwrap());
        assert!(!prompt.arguments.as_ref().unwrap()[1].required.unwrap());
    }

    #[test]
    fn test_prompt_message() {
        let user = PromptMessage::user("Hello!");
        assert!(matches!(user.role, Role::User));

        let assistant = PromptMessage::assistant("Hi there!");
        assert!(matches!(assistant.role, Role::Assistant));
    }

    #[test]
    fn test_prompt_result() {
        let result = GetPromptResult::user("Please analyze this data")
            .description("Data analysis prompt");

        assert_eq!(result.messages.len(), 1);
        assert!(result.description.is_some());
    }

    #[test]
    fn test_prompt_output_conversion() {
        let output = PromptOutput::user("Test message");
        let result: GetPromptResult = output.into();
        assert_eq!(result.messages.len(), 1);
        assert!(matches!(result.messages[0].role, Role::User));
    }
}
