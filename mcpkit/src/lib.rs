//! # MCP - Model Context Protocol SDK for Rust
//!
//! A Rust SDK for the Model Context Protocol that simplifies server development
//! through a unified `#[mcp_server]` macro.
//!
//! ## Features
//!
//! - **Reduced boilerplate** via unified `#[mcp_server]` macro
//! - **Type-safe state machines** via typestate pattern for connection lifecycle
//! - **Rich error handling** with context chains and miette diagnostics
//! - **Full MCP 2025-11-25 protocol coverage** including Tasks
//! - **Runtime-agnostic** async support
//!
//! ## Quick Start
//!
//! ```ignore
//! use mcpkit::prelude::*;
//! use mcpkit::transport::stdio::StdioTransport;
//!
//! struct Calculator;
//!
//! #[mcp_server(name = "calculator", version = "1.0.0")]
//! impl Calculator {
//!     #[tool(description = "Add two numbers")]
//!     async fn add(&self, a: f64, b: f64) -> ToolOutput {
//!         ToolOutput::text((a + b).to_string())
//!     }
//!
//!     #[tool(description = "Multiply two numbers")]
//!     async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
//!         ToolOutput::text((a * b).to_string())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), McpError> {
//!     let transport = StdioTransport::new();
//!     let server = ServerBuilder::new(Calculator)
//!         .with_tools(Calculator)
//!         .build();
//!     server.serve(transport).await
//! }
//! ```
//!
//! ## Comparison with rmcp
//!
//! | Aspect        | rmcp                 | This SDK               |
//! |---------------|----------------------|------------------------|
//! | Macros        | 4 interdependent     | 1 unified `#[mcp_server]` |
//! | Boilerplate   | Manual router wiring | Zero initialization    |
//! | Parameters    | `Parameters<T>` wrapper | Direct from signature  |
//! | Error types   | 3 nested layers      | 1 unified `McpError`     |
//! | Tasks         | Not implemented      | Full support           |
//!
//! ## Crate Organization
//!
//! ### Core MCP Protocol
//!
//! - [`mcpkit_core`] - Protocol types and traits (no async runtime)
//! - [`mcpkit_transport`] - Transport abstractions (stdio, HTTP, WebSocket)
//! - [`mod@mcpkit_server`] - Server implementation with composable handlers
//! - [`mcpkit_client`] - Client implementation
//! - [`mcpkit_macros`] - Procedural macros for `#[mcp_server]` etc.
//!
//! ### Forge Orchestration Layer (optional features)
//!
//! Enable with `features = ["forge"]` or individual features:
//!
//! - [`provider`] - Multi-LLM provider abstraction (OpenAI, Anthropic, Ollama)
//! - [`template`] - Compile-time validated prompt templates
//! - [`memory`] - Conversation memory management
//! - [`embedding`] - Vector storage and similarity search
//! - [`chain`] - LCEL-inspired composable pipelines
//! - [`agent`] - ReAct agent pattern with tool execution
//! - [`rag`] - Retrieval-Augmented Generation components
//! - [`eval`] - LLM evaluation and testing framework

#![deny(missing_docs)]

// Re-export all public items from core
pub use mcpkit_core::*;

// Re-export core modules explicitly for macro hygiene
// The macro generates paths like ::mcpkit::capability::ServerInfo
pub mod capability {
    //! Capability negotiation types.
    pub use mcpkit_core::capability::*;
}

pub mod types {
    //! MCP type definitions.
    pub use mcpkit_core::types::*;
}

pub mod error {
    //! Error types and handling.
    pub use mcpkit_core::error::*;
}

// Re-export server types
pub use mcpkit_server::{
    CancellationToken, CancelledFuture, CompletionHandler, Context, ContextData,
    ElicitationHandler, LogLevel, LoggingHandler, NoOpPeer, Peer, PromptHandler, ResourceHandler,
    SamplingHandler, Server, ServerBuilder, ServerHandler, TaskHandler, ToolHandler,
};

// Re-export transport types
pub use mcpkit_transport::{Transport, TransportListener, TransportMetadata};

// Re-export macros
pub use mcpkit_macros::{
    ToolInput, elicitation, mcp_client, mcp_server, on_connected, on_disconnected,
    on_prompts_list_changed, on_resource_updated, on_resources_list_changed, on_task_progress,
    on_tools_list_changed, prompt, resource, roots, sampling, tool,
};

pub mod prelude;

/// Server module re-exports.
///
/// Re-exports all types from [`mcpkit_server`].
pub mod server {
    pub use mcpkit_server::*;
}

/// Transport module re-exports.
///
/// Re-exports all types from [`mcpkit_transport`].
pub mod transport {
    pub use mcpkit_transport::*;
}

/// Client module re-exports.
///
/// Re-exports all types from [`mcpkit_client`].
#[cfg(feature = "client")]
pub mod client {
    pub use mcpkit_client::*;
}

// ============================================================================
// Forge Orchestration Layer
// ============================================================================

/// Multi-LLM provider abstraction.
///
/// Provides a unified interface for interacting with various LLM providers
/// including OpenAI, Anthropic, and Ollama.
///
/// Enable with `features = ["provider"]` or specific providers like `["provider-openai"]`.
#[cfg(feature = "provider")]
pub mod provider {
    pub use mcpkit_provider::*;
}

/// Compile-time validated prompt templates.
///
/// Type-safe prompt templates with compile-time validation of template variables.
///
/// Enable with `features = ["template"]`.
#[cfg(feature = "template")]
pub mod template {
    pub use mcpkit_template::*;
}

/// Conversation memory management.
///
/// Various strategies for managing conversation history including buffer,
/// window, token-based, and summary memory.
///
/// Enable with `features = ["memory"]`.
#[cfg(feature = "memory")]
pub mod memory {
    pub use mcpkit_memory::*;
}

/// Vector storage and similarity search.
///
/// In-memory and persistent vector stores for semantic search and RAG applications.
///
/// Enable with `features = ["embedding"]`, `["embedding-sqlite"]`, or `["embedding-postgres"]`.
#[cfg(feature = "embedding")]
pub mod embedding {
    pub use mcpkit_embedding::*;
}

/// LCEL-inspired composable pipelines.
///
/// Build complex LLM workflows using composable chain primitives.
///
/// Enable with `features = ["chain"]`.
#[cfg(feature = "chain")]
pub mod chain {
    pub use mcpkit_chain::*;
}

/// ReAct agent pattern with tool execution.
///
/// Autonomous agents that can reason, use tools, and accomplish complex tasks.
///
/// Enable with `features = ["agent"]`.
#[cfg(feature = "agent")]
pub mod agent {
    pub use mcpkit_agent::*;
}

/// Retrieval-Augmented Generation components.
///
/// Document loaders, text splitters, retrievers, and RAG pipelines.
///
/// Enable with `features = ["rag"]`.
#[cfg(feature = "rag")]
pub mod rag {
    pub use mcpkit_rag::*;
}

/// LLM evaluation and testing framework.
///
/// Metrics, test cases, and evaluation runners for assessing LLM outputs.
///
/// Enable with `features = ["eval"]`.
#[cfg(feature = "eval")]
pub mod eval {
    pub use mcpkit_eval::*;
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prelude_imports() {
        // Just verify the prelude compiles
        use crate::prelude::*;
        let _ = std::any::type_name::<McpError>();
    }
}
