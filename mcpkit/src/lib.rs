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
//! - [`mcpkit_core`] - Protocol types and traits (no async runtime)
//! - [`mcpkit_transport`] - Transport abstractions (stdio, HTTP, WebSocket)
//! - [`mod@mcpkit_server`] - Server implementation with composable handlers
//! - [`mcpkit_client`] - Client implementation
//! - [`mcpkit_macros`] - Procedural macros for `#[mcp_server]` etc.

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
pub use mcpkit_macros::{ToolInput, mcp_server, prompt, resource, tool};

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_prelude_imports() {
        // Just verify the prelude compiles
        use crate::prelude::*;
        let _ = std::any::type_name::<McpError>();
    }
}
