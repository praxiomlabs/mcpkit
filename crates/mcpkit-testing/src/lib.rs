//! Testing utilities for the MCP SDK.
//!
//! This crate provides comprehensive testing infrastructure for MCP servers
//! and clients, including:
//!
//! - **Mock servers and clients** for unit testing
//! - **Test fixtures** with pre-configured tools/resources
//! - **Custom assertions** for MCP-specific scenarios
//! - **Scenario runner** for defining and executing test scenarios
//! - **Async helpers** for testing async MCP code
//! - **Session testing** with recording and validation
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
//! ## Mock Client
//!
//! ```rust
//! use mcpkit_testing::MockClient;
//!
//! let client = MockClient::new()
//!     .with_info("test-client", "1.0.0");
//!
//! let request = client.create_ping_request();
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
//!
//! ## Test Scenarios
//!
//! ```rust
//! use mcpkit_testing::scenario::{TestScenario, ResponseMatcher};
//! use mcpkit_core::protocol::Request;
//!
//! let scenario = TestScenario::new("ping-test")
//!     .request(
//!         Request::new("ping", 1),
//!         ResponseMatcher::success(),
//!     );
//! ```
//!
//! ## Session Testing
//!
//! ```rust
//! use mcpkit_testing::session::TestSession;
//! use mcpkit_core::protocol::{Message, Request, Response, RequestId};
//!
//! let session = TestSession::new("my-test");
//! session.record_outbound(Message::Request(Request::new("ping", 1)));
//! session.record_inbound(Message::Response(Response::success(
//!     RequestId::from(1),
//!     serde_json::json!({}),
//! )));
//! let result = session.finalize();
//! result.assert_valid();
//! ```

#![deny(missing_docs)]

pub mod assertions;
pub mod async_helpers;
pub mod client;
pub mod fixtures;
pub mod mock;
pub mod scenario;
pub mod session;

// Re-export commonly used types
pub use assertions::{assert_tool_error, assert_tool_success};
pub use client::MockClient;
pub use fixtures::{sample_resources, sample_tools};
pub use mock::{MockServer, MockServerBuilder, MockTool};
pub use scenario::{ResponseMatcher, TestScenario};
pub use session::{TestSession, TestSessionResult};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::assertions::{assert_tool_error, assert_tool_success};
    pub use crate::async_helpers::{
        TestBarrier, TestLatch, retry, wait_for, with_default_timeout, with_timeout,
    };
    pub use crate::client::MockClient;
    pub use crate::fixtures::{sample_resources, sample_tools};
    pub use crate::mock::{MockPrompt, MockResource, MockServer, MockServerBuilder, MockTool};
    pub use crate::scenario::{
        MessageQueue, NotificationMatcher, ResponseMatcher, TestScenario, TestStep,
    };
    pub use crate::session::{TestSession, TestSessionBuilder, TestSessionResult};
}
