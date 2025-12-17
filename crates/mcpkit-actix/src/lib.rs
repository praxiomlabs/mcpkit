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
//! # HTTP Protocol Requirements
//!
//! Clients must include the `Mcp-Protocol-Version` header in all requests:
//!
//! ```text
//! POST /mcp HTTP/1.1
//! Content-Type: application/json
//! Mcp-Protocol-Version: 2025-11-25
//!
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
//! ```
//!
//! Supported protocol versions: `2024-11-05`, `2025-03-26`, `2025-06-18`, `2025-11-25`
//!
//! # Example
//!
//! ```ignore
//! use mcpkit::prelude::*;
//! use mcpkit_actix::{McpConfig, handle_mcp_post, handle_sse};
//! use actix_web::{web, App, HttpServer};
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
//! #[actix_web::main]
//! async fn main() -> std::io::Result<()> {
//!     // Create MCP config - no Clone required on handler!
//!     let config = McpConfig::new(MyServer::new());
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
//!
//! # Client Example (curl)
//!
//! ```bash
//! # Initialize the connection
//! curl -X POST http://localhost:3000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "Mcp-Protocol-Version: 2025-11-25" \
//!   -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"test","version":"1.0"},"capabilities":{}}}'
//!
//! # List available tools
//! curl -X POST http://localhost:3000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "Mcp-Protocol-Version: 2025-11-25" \
//!   -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
//!
//! # Call a tool
//! curl -X POST http://localhost:3000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "Mcp-Protocol-Version: 2025-11-25" \
//!   -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hello","arguments":{"name":"World"}}}'
//! ```

#![warn(missing_docs)]

mod error;
mod handler;
mod session;
mod state;

pub use error::ExtensionError;
pub use handler::{handle_mcp_post, handle_sse};
pub use session::{Session, SessionManager, SessionStore};
pub use state::McpConfig;

/// Protocol versions supported by this extension.
pub const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// Check if a protocol version is supported.
#[must_use]
pub fn is_supported_version(version: Option<&str>) -> bool {
    version.is_some_and(|v| SUPPORTED_VERSIONS.contains(&v))
}
