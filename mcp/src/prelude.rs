//! Prelude module for convenient imports.
//!
//! Import everything you need with a single use statement:
//!
//! ```ignore
//! use mcp::prelude::*;
//! ```
//!
//! This module re-exports the most commonly used types from the MCP SDK,
//! making it easy to get started without having to import individual items.
//!
//! ## Included Types
//!
//! ### Core Types
//! - Protocol types (Request, Response, Notification, Message)
//! - Error types (McpError, JsonRpcError)
//! - Capability types (ServerCapabilities, ClientCapabilities)
//! - Result types (CallToolResult, GetPromptResult, etc.)
//!
//! ### Server Types
//! - Handler traits (ToolHandler, ResourceHandler, PromptHandler, etc.)
//! - Server and ServerBuilder
//! - Context and ContextData
//!
//! ### Transport Types
//! - Transport trait
//! - TransportListener trait
//! - TransportMetadata
//!
//! ### Macros
//! - `#[mcp_server]` - Main server macro
//! - `#[tool]` - Tool attribute
//! - `#[resource]` - Resource attribute
//! - `#[prompt]` - Prompt attribute
//! - `#[derive(ToolInput)]` - Parameter struct derive

// Core types
pub use mcp_core::prelude::*;

// Server types
pub use mcp_server::{
    CompletionHandler, Context, ContextData, ElicitationHandler, LogLevel, LoggingHandler,
    PromptHandler, ResourceHandler, SamplingHandler, Server, ServerBuilder, ServerHandler,
    TaskHandler, ToolHandler,
};

// Transport types
pub use mcp_transport::{Transport, TransportListener, TransportMetadata};

// Macros - these are automatically available at crate root
pub use mcp_macros::{mcp_server, prompt, resource, tool, ToolInput};
