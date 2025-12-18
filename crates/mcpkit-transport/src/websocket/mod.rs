//! WebSocket transport for MCP.
//!
//! This module provides a first-class WebSocket transport implementation,
//! offering bidirectional real-time communication between MCP clients and servers.
//!
//! # Features
//!
//! - Full-duplex bidirectional communication
//! - Automatic ping/pong handling for connection health
//! - Reconnection with exponential backoff
//! - Message framing and fragmentation handling
//! - TLS/SSL support via rustls
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::websocket::WebSocketConfig;
//! use std::time::Duration;
//!
//! // Configure a WebSocket connection
//! let config = WebSocketConfig::new("ws://localhost:8080/mcp")
//!     .with_connect_timeout(Duration::from_secs(30))
//!     .with_ping_interval(Duration::from_secs(30))
//!     .with_max_reconnect_attempts(5);
//!
//! assert_eq!(config.url, "ws://localhost:8080/mcp");
//! assert!(config.auto_reconnect);
//! ```

mod client;
mod config;
mod server;

// Re-export public types
pub use client::{WebSocketTransport, WebSocketTransportBuilder};
pub use config::{ConnectionState, ExponentialBackoff, WebSocketConfig};
pub use server::{WebSocketListener, WebSocketServerConfig};

#[cfg(feature = "websocket")]
pub use server::AcceptedConnection;
