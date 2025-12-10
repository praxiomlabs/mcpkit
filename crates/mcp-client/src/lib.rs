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
//! ```ignore
//! use mcp_client::{Client, ClientBuilder};
//! use mcp_transport::StdioTransport;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), mcp_core::error::McpError> {
//!     let transport = StdioTransport::spawn("my-server", &[]).await?;
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
//! ```ignore
//! use mcp_client::ClientHandler;
//! use mcp_core::types::{CreateMessageRequest, CreateMessageResult};
//!
//! struct MyHandler;
//!
//! impl ClientHandler for MyHandler {
//!     async fn create_message(&self, request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
//!         // Handle sampling request
//!         todo!()
//!     }
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

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
