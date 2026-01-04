//! Core Memory trait definition.

use async_trait::async_trait;
use mcpkit_provider::Message;

use crate::error::MemoryResult;

/// Core trait for conversation memory implementations.
///
/// Memory implementations store and retrieve conversation history,
/// enabling context management across multiple LLM interactions.
///
/// # Thread Safety
///
/// All memory implementations must be `Send + Sync` to allow use
/// across async tasks and thread pools.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_memory::{Memory, BufferMemory};
/// use mcpkit_provider::Message;
///
/// async fn use_memory(memory: &mut impl Memory) -> MemoryResult<()> {
///     memory.add(Message::user("Hello")).await?;
///
///     let messages = memory.messages().await?;
///     println!("Message count: {}", messages.len());
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait Memory: Send + Sync {
    /// Add a message to memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the message cannot be stored (e.g., capacity exceeded).
    async fn add(&mut self, message: Message) -> MemoryResult<()>;

    /// Add multiple messages to memory.
    ///
    /// This is more efficient than calling `add` multiple times for
    /// implementations that batch operations.
    ///
    /// # Errors
    ///
    /// Returns an error if any message cannot be stored.
    async fn add_many(&mut self, messages: Vec<Message>) -> MemoryResult<()> {
        for message in messages {
            self.add(message).await?;
        }
        Ok(())
    }

    /// Get all messages from memory.
    ///
    /// Returns messages in chronological order (oldest first).
    ///
    /// # Errors
    ///
    /// Returns an error if messages cannot be retrieved.
    async fn messages(&self) -> MemoryResult<Vec<Message>>;

    /// Get messages that fit within a token budget.
    ///
    /// Returns the most recent messages that fit within the specified
    /// token limit, preserving the system message if present.
    ///
    /// # Arguments
    ///
    /// * `max_tokens` - Maximum number of tokens allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if messages cannot be retrieved or tokens cannot be estimated.
    async fn messages_within_tokens(&self, max_tokens: usize) -> MemoryResult<Vec<Message>>;

    /// Clear all messages from memory.
    async fn clear(&mut self);

    /// Get the number of messages in memory.
    fn len(&self) -> usize;

    /// Check if memory is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the estimated token count for all messages.
    ///
    /// This is an approximation based on character count.
    fn estimated_tokens(&self) -> usize;

    /// Get the last N messages.
    ///
    /// Returns messages in chronological order (oldest first within the slice).
    async fn last_n(&self, n: usize) -> MemoryResult<Vec<Message>>;

    /// Get the system message if one exists.
    ///
    /// Returns the first system message in memory, if any.
    async fn system_message(&self) -> MemoryResult<Option<Message>>;
}

/// Utility function to estimate tokens from text.
///
/// Uses a simple character-based heuristic (4 chars ≈ 1 token).
/// For accurate counts, use a proper tokenizer.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 characters per token on average
    text.len().div_ceil(4)
}

/// Utility function to estimate tokens in a message.
#[must_use]
pub fn estimate_message_tokens(message: &Message) -> usize {
    let content_tokens = message
        .text()
        .map_or(0, estimate_tokens);

    // Add overhead for role and structure (~4 tokens)
    content_tokens + 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // Empty string
        assert_eq!(estimate_tokens(""), 0);

        // Short string
        assert_eq!(estimate_tokens("Hi"), 1);

        // Typical sentence (~40 chars = ~10 tokens)
        let sentence = "Hello, how are you doing today?";
        let tokens = estimate_tokens(sentence);
        assert!(tokens >= 5 && tokens <= 15);
    }

    #[test]
    fn test_estimate_message_tokens() {
        let message = Message::user("Hello, world!");
        let tokens = estimate_message_tokens(&message);

        // Should include content + overhead
        assert!(tokens >= 4);
    }
}
