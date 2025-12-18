//! Testing utilities for the MCP SDK.
//!
//! This crate provides mocks, fixtures, and assertions for testing MCP servers
//! and clients. It includes:
//!
//! - Mock servers and clients for unit testing
//! - Test fixtures with pre-configured tools/resources
//! - Custom assertions for MCP-specific scenarios
//!
//! # Overview
//!
//! ## Mock Server
//!
//! ```rust
//! use mcpkit_testing::{MockServer, MockTool};
//! use mcpkit_core::types::ToolOutput;
//!
//! let server = MockServer::builder()
//!     .tool(MockTool::new("add")
//!         .description("Add two numbers")
//!         .handler(|args| Ok(ToolOutput::text("42"))))
//!     .build();
//!
//! // Use in tests with MemoryTransport
//! ```
//!
//! ## Test Fixtures
//!
//! ```rust
//! use mcpkit_testing::fixtures;
//!
//! let tools = fixtures::sample_tools();
//! let resources = fixtures::sample_resources();
//! ```
//!
//! ## Assertions
//!
//! ```rust
//! use mcpkit_testing::assert_tool_result;
//! use mcpkit_core::types::CallToolResult;
//!
//! let result = CallToolResult::text("42");
//! assert_tool_result!(result, "42");
//! ```

#![deny(missing_docs)]

pub mod assertions;
pub mod fixtures;
pub mod mock;

// Re-export commonly used types
pub use assertions::{assert_tool_error, assert_tool_success};
pub use fixtures::{sample_resources, sample_tools};
pub use mock::{MockServer, MockServerBuilder, MockTool};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::assertions::{assert_tool_error, assert_tool_success};
    pub use crate::fixtures::{sample_resources, sample_tools};
    pub use crate::mock::{MockServer, MockServerBuilder, MockTool};
}
