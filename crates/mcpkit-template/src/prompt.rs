//! Prompt building utilities.
//!
//! This module provides types for building conversation prompts from templates.

use serde::{Deserialize, Serialize};

/// Message role in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message that sets context/behavior.
    System,
    /// User message.
    User,
    /// Assistant response.
    Assistant,
    /// Tool/function result.
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// A message in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message sender.
    pub role: Role,
    /// The content of the message.
    pub content: String,
}

impl Message {
    /// Create a new message.
    #[must_use]
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    /// Create a system message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a user message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Create a tool result message.
    #[must_use]
    pub fn tool(content: impl Into<String>) -> Self {
        Self::new(Role::Tool, content)
    }

    /// Check if this is a system message.
    #[must_use]
    pub const fn is_system(&self) -> bool {
        matches!(self.role, Role::System)
    }

    /// Check if this is a user message.
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self.role, Role::User)
    }

    /// Check if this is an assistant message.
    #[must_use]
    pub const fn is_assistant(&self) -> bool {
        matches!(self.role, Role::Assistant)
    }
}

/// Builder for constructing conversation prompts.
///
/// # Example
///
/// ```
/// use mcpkit_template::{PromptBuilder, Role};
///
/// let messages = PromptBuilder::new()
///     .system("You are a helpful assistant.")
///     .user("What is 2 + 2?")
///     .build();
///
/// assert_eq!(messages.len(), 2);
/// assert_eq!(messages[0].role, Role::System);
/// assert_eq!(messages[1].role, Role::User);
/// ```
#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    messages: Vec<Message>,
}

impl PromptBuilder {
    /// Create a new prompt builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder with an initial system message.
    #[must_use]
    pub fn with_system(content: impl Into<String>) -> Self {
        Self::new().system(content)
    }

    /// Add a system message.
    #[must_use]
    pub fn system(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::system(content));
        self
    }

    /// Add a user message.
    #[must_use]
    pub fn user(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::user(content));
        self
    }

    /// Add an assistant message.
    #[must_use]
    pub fn assistant(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::assistant(content));
        self
    }

    /// Add a tool result message.
    #[must_use]
    pub fn tool(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message::tool(content));
        self
    }

    /// Add a message with a specific role.
    #[must_use]
    pub fn message(mut self, role: Role, content: impl Into<String>) -> Self {
        self.messages.push(Message::new(role, content));
        self
    }

    /// Add multiple messages.
    #[must_use]
    pub fn messages(mut self, messages: impl IntoIterator<Item = Message>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// Append another prompt builder's messages.
    #[must_use]
    pub fn append(mut self, other: Self) -> Self {
        self.messages.extend(other.messages);
        self
    }

    /// Get the current number of messages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if the builder has no messages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Build the final list of messages.
    #[must_use]
    pub fn build(self) -> Vec<Message> {
        self.messages
    }

    /// Build and return a reference to the messages.
    #[must_use]
    pub fn as_messages(&self) -> &[Message] {
        &self.messages
    }
}

/// A prompt template that can be reused with different contexts.
///
/// This is useful for creating reusable prompt patterns that can be
/// customized with different few-shot examples or context.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    /// The base system message.
    pub system: Option<String>,
    /// Example exchanges (few-shot learning).
    pub examples: Vec<(String, String)>,
    /// Suffix to add after user message.
    pub suffix: Option<String>,
}

impl PromptTemplate {
    /// Create a new prompt template.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the system message.
    #[must_use]
    pub fn system(mut self, content: impl Into<String>) -> Self {
        self.system = Some(content.into());
        self
    }

    /// Add a few-shot example.
    #[must_use]
    pub fn example(mut self, user: impl Into<String>, assistant: impl Into<String>) -> Self {
        self.examples.push((user.into(), assistant.into()));
        self
    }

    /// Set a suffix to append after the user message.
    #[must_use]
    pub fn suffix(mut self, content: impl Into<String>) -> Self {
        self.suffix = Some(content.into());
        self
    }

    /// Apply the template to a user query.
    #[must_use]
    pub fn apply(&self, query: impl Into<String>) -> PromptBuilder {
        let mut builder = PromptBuilder::new();

        // Add system message if present
        if let Some(system) = &self.system {
            builder = builder.system(system.clone());
        }

        // Add few-shot examples
        for (user, assistant) in &self.examples {
            builder = builder.user(user.clone()).assistant(assistant.clone());
        }

        // Add the actual query
        let mut query = query.into();
        if let Some(suffix) = &self.suffix {
            query.push_str(suffix);
        }
        builder = builder.user(query);

        builder
    }
}

impl Default for PromptTemplate {
    fn default() -> Self {
        Self {
            system: None,
            examples: Vec::new(),
            suffix: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::system("Hello");
        assert!(msg.is_system());
        assert!(!msg.is_user());
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_prompt_builder_basic() {
        let messages = PromptBuilder::new()
            .system("System prompt")
            .user("User query")
            .assistant("Response")
            .build();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_prompt_template() {
        let template = PromptTemplate::new()
            .system("You are a translator.")
            .example("Hello", "Bonjour")
            .example("Goodbye", "Au revoir");

        let messages = template.apply("Thank you").build();

        assert_eq!(messages.len(), 6);
        assert!(messages[0].is_system());
        assert_eq!(messages[5].content, "Thank you");
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }
}
