//! Token-based memory implementation.

use async_trait::async_trait;
use mcpkit_provider::{Message, Role};
use std::collections::VecDeque;

use crate::error::MemoryResult;
use crate::memory::{Memory, estimate_message_tokens};

/// A memory that manages messages within a token budget.
///
/// `TokenMemory` keeps messages that fit within a specified token limit.
/// When adding a message would exceed the limit, older messages are
/// removed to make room.
///
/// The system message (if any) is preserved separately and its tokens
/// count toward the budget.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_memory::{Memory, TokenMemory};
/// use mcpkit_provider::Message;
///
/// // Keep messages within 1000 tokens
/// let mut memory = TokenMemory::new(1000);
///
/// memory.add(Message::user("Hello!")).await?;
/// memory.add(Message::assistant("Hi there!")).await?;
///
/// // Won't exceed 1000 tokens
/// assert!(memory.estimated_tokens() <= 1000);
/// ```
#[derive(Debug, Clone)]
pub struct TokenMemory {
    /// Messages in the memory.
    messages: VecDeque<Message>,
    /// System message (preserved separately).
    system: Option<Message>,
    /// Maximum tokens allowed.
    max_tokens: usize,
    /// Current token count.
    current_tokens: usize,
    /// Tokens used by system message.
    system_tokens: usize,
}

impl TokenMemory {
    /// Create a new token memory with the given token limit.
    ///
    /// # Arguments
    ///
    /// * `max_tokens` - Maximum number of tokens to keep.
    #[must_use]
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            system: None,
            max_tokens,
            current_tokens: 0,
            system_tokens: 0,
        }
    }

    /// Create a token memory with a system message.
    #[must_use]
    pub fn with_system(max_tokens: usize, system: impl Into<String>) -> Self {
        let system_msg = Message::system(system);
        let system_tokens = estimate_message_tokens(&system_msg);

        Self {
            messages: VecDeque::new(),
            system: Some(system_msg),
            max_tokens,
            current_tokens: system_tokens,
            system_tokens,
        }
    }

    /// Get the maximum token limit.
    #[must_use]
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Get the current token count.
    #[must_use]
    pub fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    /// Get the remaining token capacity.
    #[must_use]
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.current_tokens)
    }

    /// Set a new token limit.
    ///
    /// If the new limit is smaller than the current token count,
    /// older messages will be evicted.
    pub fn set_max_tokens(&mut self, max_tokens: usize) {
        self.max_tokens = max_tokens;
        self.evict_to_fit(0);
    }

    /// Evict messages until there's room for `needed` additional tokens.
    fn evict_to_fit(&mut self, needed: usize) {
        let target = self.max_tokens.saturating_sub(needed);

        while self.current_tokens > target && !self.messages.is_empty() {
            if let Some(msg) = self.messages.pop_front() {
                let tokens = estimate_message_tokens(&msg);
                self.current_tokens = self.current_tokens.saturating_sub(tokens);
            }
        }
    }
}

#[async_trait]
impl Memory for TokenMemory {
    async fn add(&mut self, message: Message) -> MemoryResult<()> {
        if message.role == Role::System {
            // Remove old system tokens from count
            self.current_tokens = self.current_tokens.saturating_sub(self.system_tokens);

            // Store new system message
            let tokens = estimate_message_tokens(&message);
            self.system = Some(message);
            self.system_tokens = tokens;
            self.current_tokens += tokens;

            // Evict if over budget
            self.evict_to_fit(0);
        } else {
            let tokens = estimate_message_tokens(&message);

            // Evict old messages to make room
            self.evict_to_fit(tokens);

            // Add the message
            self.messages.push_back(message);
            self.current_tokens += tokens;
        }
        Ok(())
    }

    async fn messages(&self) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::with_capacity(self.messages.len() + 1);

        if let Some(sys) = &self.system {
            result.push(sys.clone());
        }

        result.extend(self.messages.iter().cloned());

        Ok(result)
    }

    async fn messages_within_tokens(&self, max_tokens: usize) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::new();
        let mut total_tokens = 0;

        // Include system if it fits
        if let Some(sys) = &self.system {
            let sys_tokens = estimate_message_tokens(sys);
            if sys_tokens <= max_tokens {
                result.push(sys.clone());
                total_tokens += sys_tokens;
            }
        }

        // Add messages from most recent
        for message in self.messages.iter().rev() {
            let msg_tokens = estimate_message_tokens(message);
            if total_tokens + msg_tokens <= max_tokens {
                let pos = usize::from(!result.is_empty());
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
        self.current_tokens = 0;
        self.system_tokens = 0;
    }

    fn len(&self) -> usize {
        self.messages.len() + usize::from(self.system.is_some())
    }

    fn estimated_tokens(&self) -> usize {
        self.current_tokens
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
    async fn test_token_memory_basic() {
        let mut memory = TokenMemory::new(100);

        memory.add(Message::user("Hello")).await.unwrap();
        memory.add(Message::assistant("Hi")).await.unwrap();

        assert_eq!(memory.len(), 2);
        assert!(memory.current_tokens() > 0);
        assert!(memory.current_tokens() <= 100);
    }

    #[tokio::test]
    async fn test_token_memory_eviction() {
        // Very small token limit
        let mut memory = TokenMemory::new(50);

        // Add messages until we exceed the limit
        memory.add(Message::user("Message one here")).await.unwrap();
        memory.add(Message::user("Message two here")).await.unwrap();
        memory.add(Message::user("Message three")).await.unwrap();
        memory.add(Message::user("Message four")).await.unwrap();

        // Should have evicted some messages
        assert!(memory.current_tokens() <= 50);
    }

    #[tokio::test]
    async fn test_token_memory_system_preserved() {
        let mut memory = TokenMemory::with_system(100, "You are helpful.");

        memory.add(Message::user("Hello")).await.unwrap();
        memory.add(Message::user("World")).await.unwrap();

        let messages = memory.messages().await.unwrap();
        assert_eq!(messages[0].role, Role::System);
    }

    #[tokio::test]
    async fn test_token_memory_remaining() {
        let mut memory = TokenMemory::new(100);
        let initial_remaining = memory.remaining_tokens();
        assert_eq!(initial_remaining, 100);

        memory.add(Message::user("Hello")).await.unwrap();
        assert!(memory.remaining_tokens() < initial_remaining);
    }

    #[tokio::test]
    async fn test_token_memory_resize() {
        let mut memory = TokenMemory::new(200);

        memory
            .add(Message::user("A long message here"))
            .await
            .unwrap();
        memory.add(Message::user("Another message")).await.unwrap();

        let before = memory.len();

        // Shrink to force eviction
        memory.set_max_tokens(20);

        // Should have fewer messages now
        assert!(memory.len() <= before);
        assert!(memory.current_tokens() <= 20);
    }
}
