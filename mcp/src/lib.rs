//! # MCP - Model Context Protocol SDK for Rust
//!
//! A production-grade Rust SDK for the Model Context Protocol that dramatically
//! reduces boilerplate compared to rmcp through a unified `#[mcp_server]` macro.
//!
//! ## Features
//!
//! - **66% less boilerplate** via unified `#[mcp_server]` macro
//! - **Type-safe state machines** via typestate pattern for connection lifecycle
//! - **Rich error handling** with context chains and miette diagnostics
//! - **Full MCP 2025-11-25 protocol coverage** including Tasks
//! - **Runtime-agnostic** async support
//!
//! ## Quick Start
//!
//! ```ignore
//! use mcp::prelude::*;
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
//!     Calculator.serve_stdio().await
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
//! | Error types   | 3 nested layers      | 1 unified McpError     |
//! | Tasks         | Not implemented      | Full support           |
//!
//! ## Crate Organization
//!
//! - [`mcp_core`] - Protocol types and traits (no async runtime)
//! - [`mcp_transport`] - Transport abstractions (stdio, HTTP, WebSocket)
//! - [`mod@mcp_server`] - Server implementation with composable handlers
//! - [`mcp_client`] - Client implementation
//! - [`mcp_macros`] - Procedural macros for `#[mcp_server]` etc.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Re-export all public items from core
pub use mcp_core::*;

// Re-export server types
pub use mcp_server::{
    CancellationToken, CancelledFuture, CompletionHandler, Context, ContextData,
    ElicitationHandler, LogLevel, LoggingHandler, NoOpPeer, Peer, PromptHandler,
    ResourceHandler, SamplingHandler, Server, ServerBuilder, ServerHandler, TaskHandler,
    ToolHandler,
};

// Re-export transport types
pub use mcp_transport::{Transport, TransportListener, TransportMetadata};

// Re-export macros
pub use mcp_macros::{mcp_server, prompt, resource, tool, ToolInput};

pub mod prelude;

/// Server module re-exports
pub mod server {
    //! Server implementation types.
    pub use mcp_server::*;
}

/// Transport module re-exports
pub mod transport {
    //! Transport layer types.
    pub use mcp_transport::*;
}

/// Client module re-exports
#[cfg(feature = "client")]
pub mod client {
    //! Client implementation types.
    pub use mcp_client::*;
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
