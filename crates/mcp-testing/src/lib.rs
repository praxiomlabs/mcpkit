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
//! ```ignore
//! use mcp_testing::{MockServer, MockTool};
//!
//! let server = MockServer::new()
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
//! ```ignore
//! use mcp_testing::fixtures;
//!
//! let tools = fixtures::standard_tools();
//! let resources = fixtures::sample_resources();
//! ```
//!
//! ## Assertions
//!
//! ```ignore
//! use mcp_testing::assert_tool_result;
//!
//! let result = client.call_tool("add", args).await?;
//! assert_tool_result!(result, "42");
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

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
