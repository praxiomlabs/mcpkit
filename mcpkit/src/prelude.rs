//! Prelude module for convenient imports.
//!
//! Import everything you need with a single use statement:
//!
//! ```rust
//! use mcpkit::prelude::*;
//!
//! // Now you have access to all common types
//! let info = ServerInfo::new("my-server", "1.0.0");
//! let caps = ServerCapabilities::new().with_tools();
//! ```
//!
//! This module re-exports the most commonly used types from the MCP SDK,
//! making it easy to get started without having to import individual items.
//!
//! ## Included Types
//!
//! ### Core Types
//! - Protocol types (Request, Response, Notification, Message)
//! - Error types (`McpError`, `JsonRpcError`)
//! - Capability types (`ServerCapabilities`, `ClientCapabilities`)
//! - Result types (`CallToolResult`, `GetPromptResult`, etc.)
//!
//! ### Server Types
//! - Handler traits (`ToolHandler`, `ResourceHandler`, `PromptHandler`, etc.)
//! - Server and `ServerBuilder`
//! - Context and `ContextData`
//!
//! ### Transport Types
//! - Transport trait
//! - `TransportListener` trait
//! - `TransportMetadata`
//!
//! ### Macros
//! - `#[mcp_server]` - Main server macro
//! - `#[tool]` - Tool attribute
//! - `#[resource]` - Resource attribute
//! - `#[prompt]` - Prompt attribute
//! - `#[derive(ToolInput)]` - Parameter struct derive

// Core types
pub use mcpkit_core::prelude::*;

// Server types
pub use mcpkit_server::{
    CompletionHandler, Context, ContextData, ElicitationHandler, LogLevel, LoggingHandler,
    PromptHandler, ResourceHandler, SamplingHandler, Server, ServerBuilder, ServerHandler,
    TaskHandler, ToolHandler,
};

// Transport types
pub use mcpkit_transport::{Transport, TransportListener, TransportMetadata};

// Macros - these are automatically available at crate root
pub use mcpkit_macros::{ToolInput, mcp_server, prompt, resource, tool};
