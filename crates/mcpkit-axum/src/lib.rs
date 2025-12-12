//! Axum integration for the Rust MCP SDK.
//!
//! This crate provides integration between the MCP SDK and the Axum web framework,
//! making it easy to expose MCP servers over HTTP.
//!
//! # Features
//!
//! - HTTP POST endpoint for JSON-RPC messages
//! - Server-Sent Events (SSE) streaming for notifications
//! - Session management with automatic cleanup
//! - Protocol version validation
//! - CORS support
//!
//! # Example
//!
//! ```ignore
//! use mcpkit_axum::{McpRouter, McpState};
//! use mcpkit_server::ServerHandler;
//! use axum::Router;
//!
//! // Your MCP server handler (must implement ServerHandler)
//! struct MyServer;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create MCP router with your handler
//!     let mcp_router = McpRouter::new(MyServer);
//!
//!     // Build the full application
//!     let app = Router::new()
//!         .nest("/mcp", mcp_router.into_router());
//!
//!     // Run the server
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```

#![warn(missing_docs)]

mod error;
mod handler;
mod router;
mod session;
mod state;

pub use error::ExtensionError;
pub use handler::{handle_mcp_post, handle_sse};
pub use router::McpRouter;
pub use session::{Session, SessionManager, SessionStore};
pub use state::McpState;

/// Protocol versions supported by this extension.
pub const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Check if a protocol version is supported.
#[must_use]
pub fn is_supported_version(version: Option<&str>) -> bool {
    version.is_some_and(|v| SUPPORTED_VERSIONS.contains(&v))
}
