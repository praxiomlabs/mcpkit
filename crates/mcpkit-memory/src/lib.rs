//! # mcpkit-memory
//!
//! Conversation memory management for the mcpkit-forge orchestration layer.
//!
//! This crate provides various memory strategies for managing conversation history
//! in LLM applications, enabling context management across multiple interactions.
//!
//! # Memory Types
//!
//! | Type | Description | Use Case |
//! |------|-------------|----------|
//! | [`BufferMemory`] | Stores all messages | Short conversations |
//! | [`WindowMemory`] | Sliding window of N messages | Fixed context window |
//! | [`TokenMemory`] | Keeps messages within token budget | Token-limited models |
//! | [`SummaryMemory`] | Summarizes older messages | Long conversations |
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_memory::{Memory, BufferMemory};
//! use mcpkit_provider::Message;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut memory = BufferMemory::new();
//!
//!     // Add messages to memory
//!     memory.add(Message::user("Hello!")).await?;
//!     memory.add(Message::assistant("Hi there! How can I help?")).await?;
//!     memory.add(Message::user("What's the weather?")).await?;
//!
//!     // Retrieve messages for context
//!     let messages = memory.messages().await?;
//!     println!("Context: {} messages", messages.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! # Choosing a Memory Type
//!
//! - **`BufferMemory`**: Simplest option, stores everything. Good for short
//!   conversations or when you control the conversation length.
//!
//! - **`WindowMemory`**: Keeps the last N messages. Useful when only recent
//!   context matters.
//!
//! - **`TokenMemory`**: Manages messages to stay within a token budget.
//!   Essential for production systems with token limits.
//!
//! - **`SummaryMemory`**: Uses an LLM to summarize older messages. Best for
//!   long conversations where historical context matters but full history
//!   would exceed limits.

#![deny(missing_docs)]

mod buffer;
mod error;
mod memory;
mod summary;
mod token;
mod window;

pub use buffer::BufferMemory;
pub use error::{MemoryError, MemoryResult};
pub use memory::Memory;
pub use summary::SummaryMemory;
pub use token::TokenMemory;
pub use window::WindowMemory;

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::buffer::BufferMemory;
    pub use crate::error::{MemoryError, MemoryResult};
    pub use crate::memory::Memory;
    pub use crate::summary::SummaryMemory;
    pub use crate::token::TokenMemory;
    pub use crate::window::WindowMemory;
}
