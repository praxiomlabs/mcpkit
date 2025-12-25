//! Warp integration for the Rust MCP SDK.
//!
//! This crate provides integration between the MCP SDK and the Warp web framework,
//! making it easy to expose MCP servers over HTTP.
//!
//! # Features
//!
//! - HTTP POST endpoint for JSON-RPC messages
//! - Server-Sent Events (SSE) streaming for notifications
//! - Session management with automatic cleanup
//! - Protocol version validation
//! - CORS support via Warp filters
//!
//! # Quick Start
//!
//! ```ignore
//! use mcpkit::prelude::*;
//! use mcpkit_warp::McpRouter;
//!
//! // Your MCP server handler (use #[mcp_server] macro)
//! #[mcp_server(name = "my-server", version = "1.0.0")]
//! impl MyServer {
//!     #[tool(description = "Say hello")]
//!     async fn hello(&self, name: String) -> ToolOutput {
//!         ToolOutput::text(format!("Hello, {name}!"))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     McpRouter::new(MyServer::new())
//!         .serve(([0, 0, 0, 0], 3000))
//!         .await;
//! }
//! ```

#![deny(missing_docs)]

mod error;
/// Handler module for MCP request processing.
pub mod handler;
mod router;
mod session;
mod state;

pub use error::WarpError;
pub use handler::{handle_mcp_post, handle_sse};
pub use router::McpRouter;
pub use session::{SessionManager, SessionStore};
pub use state::McpState;

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::WarpError;
    pub use crate::handler::{handle_mcp_post, handle_sse};
    pub use crate::router::McpRouter;
    pub use crate::session::{SessionManager, SessionStore};
    pub use crate::state::McpState;
}

/// Protocol versions supported by this extension.
pub const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// Check if a protocol version is supported.
#[must_use]
pub fn is_supported_version(version: Option<&str>) -> bool {
    version.is_some_and(|v| SUPPORTED_VERSIONS.contains(&v))
}
