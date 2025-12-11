//! Client implementation for the MCP SDK.
//!
//! This crate provides the client-side implementation for the Model Context
//! Protocol. It includes a fluent client API, server discovery, and connection
//! management.
//!
//! # Overview
//!
//! The MCP client allows AI applications to:
//!
//! - Connect to MCP servers via various transports
//! - Discover and invoke tools
//! - Read resources
//! - Get prompts
//! - Track long-running tasks
//!
//! # Example
//!
//! ```no_run
//! use mcpkit_client::{Client, ClientBuilder};
//! use mcpkit_transport::SpawnedTransport;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), mcpkit_core::error::McpError> {
//!     // Spawn an MCP server as a subprocess and connect via stdio
//!     let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;
//!
//!     let client = ClientBuilder::new()
//!         .name("my-client")
//!         .version("1.0.0")
//!         .build(transport)
//!         .await?;
//!
//!     // List available tools
//!     let tools = client.list_tools().await?;
//!     for tool in &tools {
//!         println!("Tool: {}", tool.name);
//!     }
//!
//!     // Call a tool
//!     let result = client.call_tool("add", serde_json::json!({
//!         "a": 1,
//!         "b": 2
//!     })).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Client Handler
//!
//! For handling server-initiated requests (sampling, elicitation), implement
//! the [`ClientHandler`] trait:
//!
//! ```rust
//! use mcpkit_client::ClientHandler;
//! use mcpkit_core::types::{CreateMessageRequest, CreateMessageResult};
//! use mcpkit_core::error::McpError;
//!
//! struct MyHandler;
//!
//! impl ClientHandler for MyHandler {
//!     // Override default methods as needed
//! }
//! ```

#![deny(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]

pub mod builder;
pub mod client;
pub mod discovery;
pub mod handler;
pub mod pool;

// Re-export commonly used types
pub use builder::ClientBuilder;
pub use client::Client;
pub use discovery::{DiscoveredServer, ServerDiscovery};
pub use handler::ClientHandler;
pub use pool::{ClientPool, ClientPoolBuilder, PoolConfig, PoolStats};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::builder::ClientBuilder;
    pub use crate::client::Client;
    pub use crate::discovery::{DiscoveredServer, ServerDiscovery};
    pub use crate::handler::ClientHandler;
    pub use crate::pool::{ClientPool, ClientPoolBuilder, PoolConfig, PoolStats};
}
