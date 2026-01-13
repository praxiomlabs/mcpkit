//! Summary memory implementation - uses LLM to summarize old messages.

use async_trait::async_trait;
use mcpkit_provider::{CompletionRequest, Message, Provider, Role};
use std::sync::Arc;

use crate::error::MemoryResult;
use crate::memory::{Memory, estimate_message_tokens};

/// A memory that summarizes older messages using an LLM.
///
/// `SummaryMemory` keeps recent messages in full and summarizes older
/// ones to stay within token limits. This preserves important historical
/// context while managing token usage.
///
/// # How It Works
///
/// 1. Recent messages (within `buffer_size`) are kept in full
/// 2. When buffer fills, older messages are summarized
/// 3. The summary becomes part of the context
/// 4. New messages are added to the buffer
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_memory::{Memory, SummaryMemory};
/// use mcpkit_provider::{Message, openai::OpenAiProvider};
///
/// let provider = OpenAiProvider::new("api-key")?;
/// let mut memory = SummaryMemory::new(provider, 10);
///
/// // Add many messages...
/// for i in 1..=20 {
///     memory.add(Message::user(format!("Message {}", i))).await?;
///     memory.add(Message::assistant("Response")).await?;
/// }
///
/// // Older messages are summarized, recent ones kept in full
/// let messages = memory.messages().await?;
/// ```
pub struct SummaryMemory<P: Provider> {
    /// The LLM provider for summarization.
    provider: Arc<P>,
    /// System message.
    system: Option<Message>,
    /// Summary of older messages.
    summary: Option<String>,
    /// Recent messages (buffer).
    buffer: Vec<Message>,
    /// Maximum messages in buffer before summarizing.
    buffer_size: usize,
    /// Maximum tokens for the summary.
    max_summary_tokens: usize,
    /// Model to use for summarization.
    summary_model: Option<String>,
}

impl<P: Provider> SummaryMemory<P> {
    /// Create a new summary memory.
    ///
    /// # Arguments
    ///
    /// * `provider` - LLM provider to use for summarization.
    /// * `buffer_size` - Number of recent messages to keep before summarizing.
    pub fn new(provider: P, buffer_size: usize) -> Self {
        Self {
            provider: Arc::new(provider),
            system: None,
            summary: None,
            buffer: Vec::new(),
            buffer_size,
            max_summary_tokens: 500,
            summary_model: None,
        }
    }

    /// Create a summary memory with a system message.
    pub fn with_system(provider: P, buffer_size: usize, system: impl Into<String>) -> Self {
        Self {
            provider: Arc::new(provider),
            system: Some(Message::system(system)),
            summary: None,
            buffer: Vec::new(),
            buffer_size,
            max_summary_tokens: 500,
            summary_model: None,
        }
    }

    /// Set the maximum tokens for summaries.
    #[must_use]
    pub fn max_summary_tokens(mut self, tokens: usize) -> Self {
        self.max_summary_tokens = tokens;
        self
    }

    /// Set the model to use for summarization.
    #[must_use]
    pub fn summary_model(mut self, model: impl Into<String>) -> Self {
        self.summary_model = Some(model.into());
        self
    }

    /// Get the current summary, if any.
    #[must_use]
    pub fn current_summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Force a summarization of the current buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call fails.
    pub async fn summarize_now(&mut self) -> MemoryResult<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Build conversation text
        let conversation = self
            .buffer
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::System => "System",
                    Role::Tool => "Tool",
                };
                format!("{}: {}", role, m.text().unwrap_or(""))
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Include previous summary if exists
        let to_summarize = if let Some(prev) = &self.summary {
            format!("Previous summary:\n{prev}\n\nNew conversation:\n{conversation}")
        } else {
            conversation
        };

        // Generate summary
        let prompt = format!(
            "Summarize the following conversation concisely, preserving key information, \
             decisions, and context that would be important for continuing the conversation. \
             Keep it under {} words.\n\n{}",
            self.max_summary_tokens / 2, // Rough word-to-token conversion
            to_summarize
        );

        let mut request = CompletionRequest::new()
            .message(Message::user(prompt))
            .max_tokens(self.max_summary_tokens as u32);

        if let Some(model) = &self.summary_model {
            request = request.model(model);
        }

        let response = self.provider.complete(request).await?;
        self.summary = response.text();

        // Clear the buffer
        self.buffer.clear();

        Ok(())
    }
}

#[async_trait]
impl<P: Provider> Memory for SummaryMemory<P> {
    async fn add(&mut self, message: Message) -> MemoryResult<()> {
        if message.role == Role::System {
            self.system = Some(message);
            return Ok(());
        }

        self.buffer.push(message);

        // Check if we need to summarize
        if self.buffer.len() >= self.buffer_size {
            self.summarize_now().await?;
        }

        Ok(())
    }

    async fn messages(&self) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::new();

        // System message first
        if let Some(sys) = &self.system {
            result.push(sys.clone());
        }

        // Add summary as a system-like context if present
        if let Some(summary) = &self.summary {
            result.push(Message::system(format!(
                "[Conversation Summary]\n{summary}"
            )));
        }

        // Add recent buffer messages
        result.extend(self.buffer.iter().cloned());

        Ok(result)
    }

    async fn messages_within_tokens(&self, max_tokens: usize) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::new();
        let mut total_tokens = 0;

        // System message
        if let Some(sys) = &self.system {
            let tokens = estimate_message_tokens(sys);
            if tokens <= max_tokens {
                result.push(sys.clone());
                total_tokens += tokens;
            }
        }

        // Summary
        if let Some(summary) = &self.summary {
            let summary_msg = Message::system(format!("[Conversation Summary]\n{summary}"));
            let tokens = estimate_message_tokens(&summary_msg);
            if total_tokens + tokens <= max_tokens {
                result.push(summary_msg);
                total_tokens += tokens;
            }
        }

        // Buffer messages from most recent
        for message in self.buffer.iter().rev() {
            let tokens = estimate_message_tokens(message);
            if total_tokens + tokens <= max_tokens {
                let pos = result.len().min(2); // After system and summary
                result.insert(pos, message.clone());
                total_tokens += tokens;
            } else {
                break;
            }
        }

        Ok(result)
    }

    async fn clear(&mut self) {
        self.system = None;
        self.summary = None;
        self.buffer.clear();
    }

    fn len(&self) -> usize {
        let mut count = self.buffer.len();
        if self.system.is_some() {
            count += 1;
        }
        if self.summary.is_some() {
            count += 1;
        }
        count
    }

    fn estimated_tokens(&self) -> usize {
        let mut tokens = 0;

        if let Some(sys) = &self.system {
            tokens += estimate_message_tokens(sys);
        }

        if let Some(summary) = &self.summary {
            // Rough estimate for summary
            tokens += summary.len() / 4 + 10;
        }

        for msg in &self.buffer {
            tokens += estimate_message_tokens(msg);
        }

        tokens
    }

    async fn last_n(&self, n: usize) -> MemoryResult<Vec<Message>> {
        let start = self.buffer.len().saturating_sub(n);
        Ok(self.buffer[start..].to_vec())
    }

    async fn system_message(&self) -> MemoryResult<Option<Message>> {
        Ok(self.system.clone())
    }
}

// Note: Tests for SummaryMemory require a mock provider, so they're in the integration tests.

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_provider::openai::OpenAiProvider;

    // Basic structure tests that don't require actual API calls
    #[test]
    fn test_summary_memory_creation() {
        // This just tests that types compile correctly
        // Actual provider calls would require mocking
        fn type_check(p: OpenAiProvider) -> SummaryMemory<OpenAiProvider> {
            SummaryMemory::new(p, 10)
        }
        // Verify the function signature compiles - we don't need to call it
        let _ = type_check as fn(_) -> _;
    }
}
