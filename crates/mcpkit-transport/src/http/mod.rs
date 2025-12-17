//! HTTP transport with Server-Sent Events (SSE) streaming.
//!
//! This module provides HTTP-based transport for MCP, supporting the Streamable
//! HTTP transport specification from MCP 2025-06-18.
//!
//! # Features
//!
//! - Standard HTTP POST requests for sending messages
//! - Server-Sent Events (SSE) for receiving streaming responses
//! - Session management with MCP session IDs
//! - Automatic reconnection with Last-Event-ID support
//! - Protocol version header handling
//!
//! # Protocol Reference
//!
//! The Streamable HTTP transport is defined in the MCP specification
//! [2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports).
//!
//! Key protocol requirements:
//! - Client sends JSON-RPC messages via HTTP POST
//! - Accept header must include both `application/json` and `text/event-stream`
//! - Server may respond with JSON or SSE stream
//! - Session ID assigned during initialization and included in subsequent requests
//! - Protocol version header required on all requests
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::http::HttpTransportConfig;
//! use std::time::Duration;
//!
//! // Configure an HTTP transport
//! let config = HttpTransportConfig::new("http://localhost:8080/mcp")
//!     .with_connect_timeout(Duration::from_secs(30))
//!     .with_request_timeout(Duration::from_secs(60))
//!     .with_max_reconnect_attempts(3);
//!
//! assert_eq!(config.base_url, "http://localhost:8080/mcp");
//! assert!(config.auto_reconnect);
//! ```

mod client;
mod config;
mod sse;

#[cfg(feature = "http")]
mod server;

// Re-export public types
pub use client::HttpTransport;
pub use config::{
    HttpTransportBuilder, HttpTransportConfig,
    DEFAULT_MAX_MESSAGE_SIZE, MCP_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION_HEADER,
    MCP_SESSION_ID_HEADER,
};

#[cfg(feature = "http")]
pub use server::{HttpServerConfig, HttpTransportListener};

// Re-export SSE types for testing
pub use sse::HttpTransportState;
