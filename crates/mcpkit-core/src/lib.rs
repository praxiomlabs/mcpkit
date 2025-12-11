//! # mcp-core
//!
//! Core types and traits for the Model Context Protocol (MCP) SDK.
//!
//! This crate provides the foundational building blocks for the MCP SDK:
//!
//! - **Protocol types**: JSON-RPC 2.0 request/response/notification types
//! - **MCP types**: Tools, resources, prompts, tasks, content, sampling, elicitation
//! - **Capability negotiation**: Client and server capabilities
//! - **Error handling**: Unified `McpError` type with rich diagnostics
//! - **Typestate connection**: Compile-time enforced connection lifecycle
//!
//! This crate is runtime-agnostic and does not depend on any async runtime.
//! It can be used with Tokio, async-std, smol, or any other executor.
//!
//! # Protocol Version
//!
//! This crate implements MCP protocol version **2025-11-25**.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::{
//!     types::{Tool, ToolOutput, Content},
//!     capability::{ServerCapabilities, ServerInfo},
//!     state::Connection,
//! };
//!
//! // Create a tool definition
//! let tool = Tool::new("search")
//!     .description("Search the database")
//!     .input_schema(serde_json::json!({
//!         "type": "object",
//!         "properties": {
//!             "query": { "type": "string" }
//!         },
//!         "required": ["query"]
//!     }));
//!
//! // Create server capabilities
//! let caps = ServerCapabilities::new()
//!     .with_tools()
//!     .with_resources()
//!     .with_tasks();
//!
//! // Create server info
//! let info = ServerInfo::new("my-server", "1.0.0");
//! ```
//!
//! # Feature Flags
//!
//! This crate currently has no optional features. All functionality is
//! included by default.

#![deny(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]

pub mod auth;
pub mod capability;
pub mod error;
pub mod protocol;
pub mod protocol_version;
pub mod schema;
pub mod state;
pub mod types;

// Re-export commonly used types at the crate root
pub use capability::{
    is_version_supported, negotiate_version, negotiate_version_detailed, ClientCapabilities,
    ClientInfo, InitializeRequest, InitializeResult, ServerCapabilities, ServerInfo,
    VersionNegotiationResult, PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
};
pub use error::{JsonRpcError, McpError, McpResultExt};
pub use protocol::{Message, Notification, ProgressToken, Request, RequestId, Response};
pub use protocol_version::ProtocolVersion;
pub use state::{Closing, Connected, Connection, Disconnected, Initializing, Ready};

/// Prelude module for convenient imports.
///
/// # Example
///
/// ```rust
/// use mcpkit_core::prelude::*;
/// ```
pub mod prelude {
    pub use crate::capability::{
        is_version_supported, negotiate_version, negotiate_version_detailed, ClientCapabilities,
        ClientInfo, InitializeRequest, InitializeResult, ServerCapabilities, ServerInfo,
        VersionNegotiationResult, PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
    };
    pub use crate::error::{McpError, McpResultExt};
    pub use crate::protocol::{Message, Notification, ProgressToken, Request, RequestId, Response};
    pub use crate::protocol_version::ProtocolVersion;
    pub use crate::schema::{Schema, SchemaBuilder, SchemaType};
    pub use crate::state::{Closing, Connected, Connection, Disconnected, Initializing, Ready};
    pub use crate::types::{
        // Content types
        Content,
        ContentAnnotations,
        Role,
        // Tool types
        CallToolResult,
        Tool,
        ToolAnnotations,
        ToolOutput,
        // Resource types
        Resource,
        ResourceContents,
        ResourceTemplate,
        // Prompt types
        GetPromptResult,
        Prompt,
        PromptArgument,
        PromptMessage,
        PromptOutput,
        // Task types
        Task,
        TaskError,
        TaskId,
        TaskProgress,
        TaskStatus,
        TaskSummary,
        // Sampling types
        CreateMessageRequest,
        CreateMessageResult,
        ModelPreferences,
        SamplingMessage,
        StopReason,
        // Elicitation types
        ElicitAction,
        ElicitRequest,
        ElicitResult,
        ElicitationSchema,
        PropertySchema,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prelude_imports() {
        use crate::prelude::*;

        // Just verify that all the types are accessible
        let _tool = Tool::new("test");
        let _caps = ServerCapabilities::new().with_tools();
        let _conn: Connection<Disconnected> = Connection::new();
    }

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "2025-11-25");
    }

    #[test]
    fn test_error_context() {
        use crate::error::McpResultExt;

        fn might_fail() -> Result<(), McpError> {
            Err(McpError::InternalMessage {
                message: "something went wrong".to_string(),
            })
        }

        let result = might_fail().context("while doing something important");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("while doing something important"));
    }
}
