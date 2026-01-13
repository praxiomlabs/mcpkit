//! Buffer memory implementation - stores all messages.

use async_trait::async_trait;
use mcpkit_provider::{Message, Role};

use crate::error::MemoryResult;
use crate::memory::{Memory, estimate_message_tokens};

/// A simple buffer that stores all messages.
///
/// `BufferMemory` keeps all messages in memory without any truncation
/// or summarization. It's suitable for:
///
/// - Short conversations with known bounds
/// - Debugging and development
/// - Cases where you manage context externally
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_memory::{Memory, BufferMemory};
/// use mcpkit_provider::Message;
///
/// let mut memory = BufferMemory::new();
///
/// memory.add(Message::user("Hello")).await?;
/// memory.add(Message::assistant("Hi there!")).await?;
///
/// assert_eq!(memory.len(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct BufferMemory {
    messages: Vec<Message>,
}

impl BufferMemory {
    /// Create a new empty buffer memory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Create a buffer memory with initial messages.
    #[must_use]
    pub fn with_messages(messages: Vec<Message>) -> Self {
        Self { messages }
    }

    /// Create a buffer memory with a system message.
    #[must_use]
    pub fn with_system(system: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::system(system)],
        }
    }
}

#[async_trait]
impl Memory for BufferMemory {
    async fn add(&mut self, message: Message) -> MemoryResult<()> {
        self.messages.push(message);
        Ok(())
    }

    async fn add_many(&mut self, messages: Vec<Message>) -> MemoryResult<()> {
        self.messages.extend(messages);
        Ok(())
    }

    async fn messages(&self) -> MemoryResult<Vec<Message>> {
        Ok(self.messages.clone())
    }

    async fn messages_within_tokens(&self, max_tokens: usize) -> MemoryResult<Vec<Message>> {
        let mut result = Vec::new();
        let mut total_tokens = 0;

        // Always include system message if present
        let system_msg = self.messages.iter().find(|m| m.role == Role::System);
        if let Some(sys) = system_msg {
            let sys_tokens = estimate_message_tokens(sys);
            if sys_tokens <= max_tokens {
                result.push(sys.clone());
                total_tokens += sys_tokens;
            }
        }

        // Add messages from most recent, working backwards
        let non_system: Vec<_> = self
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .collect();

        for message in non_system.into_iter().rev() {
            let msg_tokens = estimate_message_tokens(message);
            if total_tokens + msg_tokens <= max_tokens {
                result.insert(usize::from(!result.is_empty()), message.clone());
                total_tokens += msg_tokens;
            } else {
                break;
            }
        }

        Ok(result)
    }

    async fn clear(&mut self) {
        self.messages.clear();
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn estimated_tokens(&self) -> usize {
        self.messages.iter().map(estimate_message_tokens).sum()
    }

    async fn last_n(&self, n: usize) -> MemoryResult<Vec<Message>> {
        let start = self.messages.len().saturating_sub(n);
        Ok(self.messages[start..].to_vec())
    }

    async fn system_message(&self) -> MemoryResult<Option<Message>> {
        Ok(self
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_memory_basic() {
        let mut memory = BufferMemory::new();

        memory.add(Message::user("Hello")).await.unwrap();
        memory.add(Message::assistant("Hi")).await.unwrap();

        assert_eq!(memory.len(), 2);
        assert!(!memory.is_empty());

        let messages = memory.messages().await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_buffer_memory_with_system() {
        let memory = BufferMemory::with_system("You are helpful.");

        let system = memory.system_message().await.unwrap();
        assert!(system.is_some());
        assert_eq!(system.unwrap().text(), Some("You are helpful."));
    }

    #[tokio::test]
    async fn test_buffer_memory_clear() {
        let mut memory = BufferMemory::new();
        memory.add(Message::user("Hello")).await.unwrap();
        assert_eq!(memory.len(), 1);

        memory.clear().await;
        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
    }

    #[tokio::test]
    async fn test_buffer_memory_last_n() {
        let mut memory = BufferMemory::new();
        memory.add(Message::user("1")).await.unwrap();
        memory.add(Message::user("2")).await.unwrap();
        memory.add(Message::user("3")).await.unwrap();
        memory.add(Message::user("4")).await.unwrap();

        let last_2 = memory.last_n(2).await.unwrap();
        assert_eq!(last_2.len(), 2);
        assert_eq!(last_2[0].text(), Some("3"));
        assert_eq!(last_2[1].text(), Some("4"));
    }

    #[tokio::test]
    async fn test_buffer_memory_tokens() {
        let mut memory = BufferMemory::new();
        memory
            .add(Message::system("You are a helpful assistant."))
            .await
            .unwrap();
        memory.add(Message::user("Hello")).await.unwrap();
        memory.add(Message::assistant("Hi there!")).await.unwrap();

        // Token estimate should be reasonable
        let tokens = memory.estimated_tokens();
        assert!(tokens > 10);
    }

    #[tokio::test]
    async fn test_messages_within_tokens() {
        let mut memory = BufferMemory::new();
        memory.add(Message::system("System")).await.unwrap();
        memory.add(Message::user("Message 1")).await.unwrap();
        memory.add(Message::user("Message 2")).await.unwrap();
        memory.add(Message::user("Message 3")).await.unwrap();

        // With low token limit, should get system + most recent
        let messages = memory.messages_within_tokens(20).await.unwrap();
        assert!(messages.len() <= 3);

        // First should be system
        assert_eq!(messages[0].role, Role::System);
    }
}
