//! Transport abstractions for the MCP SDK.
//!
//! This crate provides transport layer implementations for the MCP protocol.
//! Transports handle the low-level details of sending and receiving JSON-RPC
//! messages between MCP clients and servers.
//!
//! # Overview
//!
//! The transport layer is responsible for:
//!
//! - Serializing and deserializing JSON-RPC messages
//! - Managing connection lifecycle
//! - Providing different transport implementations (stdio, HTTP, WebSocket)
//!
//! # Available Transports
//!
//! - [`stdio::StdioTransport`]: Standard I/O transport for subprocess communication
//! - [`memory::MemoryTransport`]: In-memory transport for testing
//!
//! # Runtime Support
//!
//! This crate supports multiple async runtimes through feature flags:
//!
//! - `tokio-runtime` (default): Use Tokio for async I/O
//! - `async-std-runtime`: Use async-std for async I/O
//!
//! # Example
//!
//! ```ignore
//! use mcp_transport::{Transport, stdio::StdioTransport};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let transport = StdioTransport::new();
//!
//!     // Send and receive messages
//!     while let Some(msg) = transport.recv().await? {
//!         // Handle the message
//!     }
//!
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod error;
pub mod http;
pub mod memory;
pub mod middleware;
pub mod pool;
pub mod runtime;
pub mod stdio;
pub mod telemetry;
pub mod traits;
pub mod websocket;

#[cfg(unix)]
pub mod unix;

// Re-export commonly used types
pub use error::TransportError;
pub use traits::{Transport, TransportExt, TransportListener, TransportMetadata};

// Runtime-agnostic transports - available when ANY runtime is enabled
#[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime", feature = "smol-runtime"))]
pub use memory::MemoryTransport;

// Note: StdioTransport has runtime-specific type parameters, so we re-export
// the module rather than a specific type alias
pub use stdio::SyncStdioTransport;

// HTTP transport (always export config/builder, listener only with http feature)
pub use http::{HttpTransport, HttpTransportConfig, HttpTransportBuilder};
#[cfg(feature = "http")]
pub use http::HttpTransportListener;

// WebSocket transport
pub use websocket::{
    WebSocketTransport, WebSocketConfig, WebSocketTransportBuilder, WebSocketListener,
    WebSocketServerConfig, ConnectionState, ExponentialBackoff,
};

// Unix socket transport
#[cfg(unix)]
pub use unix::{UnixTransport, UnixListener, UnixSocketConfig, UnixTransportBuilder};

// Connection pooling
pub use pool::{Pool, PoolConfig, PooledConnection, PoolStats};

// Telemetry
pub use telemetry::{
    TelemetryConfig, TelemetryMetrics, TelemetryLayer, TelemetryTransport,
    MetricsSnapshot, LatencyHistogram,
};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::TransportError;
    pub use crate::traits::{Transport, TransportExt, TransportListener, TransportMetadata};

    #[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime", feature = "smol-runtime"))]
    pub use crate::memory::MemoryTransport;

    pub use crate::stdio::SyncStdioTransport;

    // HTTP
    pub use crate::http::{HttpTransport, HttpTransportConfig};

    // WebSocket
    pub use crate::websocket::{WebSocketTransport, WebSocketConfig, WebSocketServerConfig};

    // Unix
    #[cfg(unix)]
    pub use crate::unix::{UnixTransport, UnixListener};

    // Pool
    pub use crate::pool::{Pool, PoolConfig, PooledConnection};
}
