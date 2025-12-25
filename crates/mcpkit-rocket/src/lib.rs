//! Rocket integration for the Rust MCP SDK.
//!
//! This crate provides integration between the MCP SDK and the Rocket web framework,
//! making it easy to expose MCP servers over HTTP.
//!
//! # Features
//!
//! - HTTP POST endpoint for JSON-RPC messages
//! - Server-Sent Events (SSE) streaming for notifications
//! - Session management with automatic cleanup
//! - Protocol version validation
//! - CORS support via Rocket fairings
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
//! # Quick Start
//!
//! ```ignore
//! use mcpkit::prelude::*;
//! use mcpkit_rocket::McpRouter;
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
//! #[rocket::main]
//! async fn main() -> Result<(), rocket::Error> {
//!     McpRouter::new(MyServer::new())
//!         .launch()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! # Advanced Usage
//!
//! For more control, use `into_rocket()` to integrate with an existing app:
//!
//! ```ignore
//! use mcpkit_rocket::McpRouter;
//! use rocket::{Build, Rocket};
//!
//! fn build_app() -> Rocket<Build> {
//!     let mcp = McpRouter::new(MyServer::new())
//!         .with_cors();
//!
//!     rocket::build()
//!         .attach(mcp.into_fairing())
//!         .mount("/health", routes![health_check])
//! }
//!
//! #[rocket::get("/")]
//! fn health_check() -> &'static str {
//!     "OK"
//! }
//! ```
//!
//! # Client Example (curl)
//!
//! ```bash
//! # Initialize the connection
//! curl -X POST http://localhost:8000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "Mcp-Protocol-Version: 2025-11-25" \
//!   -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"test","version":"1.0"},"capabilities":{}}}'
//!
//! # List available tools
//! curl -X POST http://localhost:8000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "Mcp-Protocol-Version: 2025-11-25" \
//!   -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
//! ```

#![deny(missing_docs)]

mod error;
/// Handler module for MCP request processing.
pub mod handler;
mod router;
mod session;
mod state;

pub use error::RocketError;
pub use handler::{
    LastEventIdHeader, McpResponse, ProtocolVersionHeader, SessionIdHeader, handle_mcp_post,
    handle_sse,
};
pub use router::{Cors, McpRouter};
pub use session::{SessionManager, SessionStore};
pub use state::McpState;

/// Prelude module for convenient imports.
///
/// # Example
///
/// ```ignore
/// use mcpkit_rocket::prelude::*;
/// ```
pub mod prelude {
    pub use crate::error::RocketError;
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
