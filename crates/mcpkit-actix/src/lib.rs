//! Actix-web integration for the Rust MCP SDK.
//!
//! This crate provides integration between the MCP SDK and the Actix-web framework,
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
//! use mcpkit_actix::{McpConfig, handle_mcp_post, handle_sse};
//! use mcpkit_server::ServerHandler;
//! use actix_web::{web, App, HttpServer};
//!
//! // Your MCP server handler (must implement ServerHandler)
//! struct MyServer;
//!
//! #[actix_web::main]
//! async fn main() -> std::io::Result<()> {
//!     // Create MCP config with your handler
//!     let config = McpConfig::new(MyServer);
//!
//!     HttpServer::new(move || {
//!         App::new()
//!             .app_data(web::Data::new(config.clone()))
//!             .route("/mcp", web::post().to(handle_mcp_post::<MyServer>))
//!             .route("/mcp/sse", web::get().to(handle_sse::<MyServer>))
//!     })
//!     .bind("0.0.0.0:3000")?
//!     .run()
//!     .await
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]

mod error;
mod handler;
mod session;
mod state;

pub use error::ExtensionError;
pub use handler::{handle_mcp_post, handle_sse};
pub use session::{Session, SessionManager, SessionStore};
pub use state::McpConfig;

/// Protocol versions supported by this extension.
pub const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Check if a protocol version is supported.
#[must_use]
pub fn is_supported_version(version: Option<&str>) -> bool {
    version
        .map(|v| SUPPORTED_VERSIONS.contains(&v))
        .unwrap_or(false)
}
