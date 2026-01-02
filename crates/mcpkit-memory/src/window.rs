//! Window memory implementation - sliding window of recent messages.

use async_trait::async_trait;
use mcpkit_provider::{Message, Role};
use std::collections::VecDeque;

use crate::error::MemoryResult;
use crate::memory::{estimate_message_tokens, Memory};

/// A sliding window memory that keeps the last N messages.
///
/// `WindowMemory` maintains a fixed-size window of the most recent
/// messages. When the window is full, older messages are dropped
/// to make room for new ones.
///
/// The system message (if any) is preserved separately and always
/// included when retrieving messages.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_memory::{Memory, WindowMemory};
/// use mcpkit_provider::Message;
///
/// let mut memory = WindowMemory::new(5); // Keep last 5 messages
///
/// for i in 1..=10 {
///     memory.add(Message::user(format!("Message {}", i))).await?;
/// }
///
/// // Only the last 5 messages are kept
/// assert_eq!(memory.len(), 5);
/// ```
#[derive(Debug, Clone)]
pub struct WindowMemory {
    /// The fixed-size window of messages.
    messages: VecDeque<Message>,
    /// System message (preserved separately).
    system: Option<Message>,
    /// Maximum number of messages to keep (excluding system).
    window_size: usize,
}

impl WindowMemory {
    /// Create a new window memory with the given window size.
    ///
    /// # Arguments
    ///
    /// * `window_size` - Maximum number of messages to keep (excluding system message).
    #[must_use]
    pub fn new(window_size: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(window_size),
            system: None,
            window_size,
        }
    }

    /// Create a window memory with a system message.
    #[must_use]
    pub fn with_system(window_size: usize, system: impl Into<String>) -> Self {
        Self {
            messages: VecDeque::with_capacity(window_size),
            system: Some(Message::system(system)),
            window_size,
        }
    }

    /// Get the window size.
    #[must_use]
    pub fn window_size(&self) -> usize {
        self.window_size
    }

    /// Set a new window size.
    ///
    /// If the new size is smaller than the current number of messages,
    /// older messages will be dropped.
    pub fn set_window_size(&mut self, size: usize) {
        self.window_size = size;
        while self.messages.len() > size {
            self.messages.pop_front();
        }
    }
}

#[async_trait]
impl Memory for WindowMemory {
    async fn add(&mut self, message: Message) -> MemoryResult<()> {
        if message.role == Role::System {
            // System messages are stored separately
            self.system = Some(message);
        } else {
            // If at capacity, remove oldest message
            if self.messages.len() >= self.window_size {
                self.messages.pop_front();
            }
            self.messages.push_back(message);
        }
        Ok(())
    }

    async fn messages(&self) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::with_capacity(self.messages.len() + 1);

        // System message first
        if let Some(sys) = &self.system {
            result.push(sys.clone());
        }

        // Then conversation messages
        result.extend(self.messages.iter().cloned());

        Ok(result)
    }

    async fn messages_within_tokens(&self, max_tokens: usize) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::new();
        let mut total_tokens = 0;

        // Always include system message if it fits
        if let Some(sys) = &self.system {
            let sys_tokens = estimate_message_tokens(sys);
            if sys_tokens <= max_tokens {
                result.push(sys.clone());
                total_tokens += sys_tokens;
            }
        }

        // Add messages from most recent, working backwards
        for message in self.messages.iter().rev() {
            let msg_tokens = estimate_message_tokens(message);
            if total_tokens + msg_tokens <= max_tokens {
                // Insert after system message
                let pos = if result.is_empty() { 0 } else { 1 };
                result.insert(pos, message.clone());
                total_tokens += msg_tokens;
            } else {
                break;
            }
        }

        Ok(result)
    }

    async fn clear(&mut self) {
        self.messages.clear();
        self.system = None;
    }

    fn len(&self) -> usize {
        self.messages.len() + usize::from(self.system.is_some())
    }

    fn estimated_tokens(&self) -> usize {
        let system_tokens = self
            .system
            .as_ref()
            .map(estimate_message_tokens)
            .unwrap_or(0);

        let message_tokens: usize = self.messages.iter().map(estimate_message_tokens).sum();

        system_tokens + message_tokens
    }

    async fn last_n(&self, n: usize) -> MemoryResult<Vec<Message>> {
        let start = self.messages.len().saturating_sub(n);
        Ok(self.messages.iter().skip(start).cloned().collect())
    }

    async fn system_message(&self) -> MemoryResult<Option<Message>> {
        Ok(self.system.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_window_memory_basic() {
        let mut memory = WindowMemory::new(3);

        memory.add(Message::user("1")).await.unwrap();
        memory.add(Message::user("2")).await.unwrap();
        memory.add(Message::user("3")).await.unwrap();

        assert_eq!(memory.len(), 3);

        // Adding a 4th message should evict the first
        memory.add(Message::user("4")).await.unwrap();
        assert_eq!(memory.len(), 3);

        let messages = memory.messages().await.unwrap();
        assert_eq!(messages[0].text(), Some("2"));
        assert_eq!(messages[2].text(), Some("4"));
    }

    #[tokio::test]
    async fn test_window_memory_system_preserved() {
        let mut memory = WindowMemory::with_system(2, "You are helpful.");

        memory.add(Message::user("1")).await.unwrap();
        memory.add(Message::user("2")).await.unwrap();
        memory.add(Message::user("3")).await.unwrap();

        // System + 2 most recent
        let messages = memory.messages().await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].text(), Some("2"));
        assert_eq!(messages[2].text(), Some("3"));
    }

    #[tokio::test]
    async fn test_window_memory_resize() {
        let mut memory = WindowMemory::new(5);

        for i in 1..=5 {
            memory.add(Message::user(format!("{i}"))).await.unwrap();
        }
        assert_eq!(memory.len(), 5);

        // Shrink window
        memory.set_window_size(2);
        assert_eq!(memory.len(), 2);

        let messages = memory.messages().await.unwrap();
        assert_eq!(messages[0].text(), Some("4"));
        assert_eq!(messages[1].text(), Some("5"));
    }

    #[tokio::test]
    async fn test_window_memory_clear() {
        let mut memory = WindowMemory::with_system(3, "System");
        memory.add(Message::user("Hello")).await.unwrap();

        memory.clear().await;

        assert!(memory.is_empty());
        assert!(memory.system_message().await.unwrap().is_none());
    }
}
