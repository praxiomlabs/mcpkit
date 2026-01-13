# mcpkit-memory

Conversation memory management for the mcpkit-forge orchestration layer.

## Overview

`mcpkit-memory` provides various memory strategies for managing conversation history in LLM applications. It enables context management across multiple interactions, supporting everything from simple buffer storage to intelligent summarization.

## Memory Types

| Type | Description | Use Case |
|------|-------------|----------|
| `BufferMemory` | Stores all messages | Short conversations |
| `WindowMemory` | Sliding window of N messages | Fixed context window |
| `TokenMemory` | Keeps messages within token budget | Token-limited models |
| `SummaryMemory` | Summarizes older messages | Long conversations |

## Quick Start

```rust
use mcpkit_memory::{Memory, BufferMemory};
use mcpkit_provider::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut memory = BufferMemory::new();

    // Add messages to memory
    memory.add(Message::user("Hello!")).await?;
    memory.add(Message::assistant("Hi there! How can I help?")).await?;
    memory.add(Message::user("What's the weather?")).await?;

    // Retrieve messages for context
    let messages = memory.messages().await?;
    println!("Context: {} messages", messages.len());

    Ok(())
}
```

## Buffer Memory

Stores all messages without limits. Best for short conversations:

```rust
use mcpkit_memory::{Memory, BufferMemory};

let mut memory = BufferMemory::new();

// Add messages
memory.add(Message::user("Hello")).await?;
memory.add(Message::assistant("Hi!")).await?;

// Clear when needed
memory.clear().await?;
```

## Window Memory

Keeps only the last N messages. Useful for fixed context windows:

```rust
use mcpkit_memory::{Memory, WindowMemory};

// Keep last 10 messages
let mut memory = WindowMemory::new(10);

// Older messages are automatically dropped
for i in 0..20 {
    memory.add(Message::user(format!("Message {}", i))).await?;
}

let messages = memory.messages().await?;
assert_eq!(messages.len(), 10);  // Only last 10 retained
```

## Token Memory

Manages messages to stay within a token budget:

```rust
use mcpkit_memory::{Memory, TokenMemory};

// Stay within 4096 tokens
let mut memory = TokenMemory::new(4096);

// Older messages are pruned to stay within budget
memory.add(Message::user("Long message...")).await?;
memory.add(Message::assistant("Another long response...")).await?;
```

## Summary Memory

Uses an LLM to summarize older messages, preserving context while staying compact:

```rust
use mcpkit_memory::{Memory, SummaryMemory};
use mcpkit_provider::openai::OpenAiProvider;

let provider = OpenAiProvider::new(api_key)?;
let mut memory = SummaryMemory::new(provider, 10)  // Keep 10 recent messages
    .summary_model("gpt-4o")        // Model for summarization
    .max_summary_tokens(500);       // Max tokens for summary

// Add messages normally
memory.add(Message::user("Hello!")).await?;
memory.add(Message::assistant("Hi!")).await?;

// When buffer fills, older messages are summarized
// The summary becomes a system message preserving context
```

## Choosing a Memory Type

- **`BufferMemory`**: Simplest option, stores everything. Good for short conversations or when you control the conversation length.

- **`WindowMemory`**: Keeps the last N messages. Useful when only recent context matters, such as quick Q&A sessions.

- **`TokenMemory`**: Manages messages to stay within a token budget. Essential for production systems with token limits.

- **`SummaryMemory`**: Uses an LLM to summarize older messages. Best for long conversations where historical context matters but full history would exceed limits.

## The Memory Trait

All memory types implement the `Memory` trait:

```rust
use mcpkit_memory::Memory;
use async_trait::async_trait;

#[async_trait]
pub trait Memory: Send + Sync {
    /// Add a message to memory
    async fn add(&mut self, message: Message) -> MemoryResult<()>;

    /// Get all messages for context
    async fn messages(&self) -> MemoryResult<Vec<Message>>;

    /// Clear all messages
    async fn clear(&mut self) -> MemoryResult<()>;
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
