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
//! | Transport | Use Case | Feature Flag |
//! |-----------|----------|--------------|
//! | [`stdio::SyncStdioTransport`] | Subprocess communication (CLI tools) | Always available |
//! | [`memory::MemoryTransport`] | Testing and in-process communication | Requires runtime feature |
//! | [`spawn::SpawnedTransport`] | Spawn MCP servers as subprocesses | `tokio-runtime` |
//! | [`http::HttpTransport`] | HTTP client for streamable HTTP servers | Always available |
//! | [`http::HttpTransportListener`] | HTTP server (Streamable HTTP) | `http` feature |
//! | [`websocket::WebSocketTransport`] | WebSocket client with reconnection | Always available |
//! | [`websocket::WebSocketListener`] | WebSocket server | Always available |
//! | `grpc::GrpcTransport` | gRPC client with bidirectional streaming | `grpc` feature |
//! | `unix::UnixTransport` | Unix domain sockets (local IPC) | Unix platforms only |
//! | `windows::NamedPipeTransport` | Windows named pipes (local IPC) | Windows only |
//!
//! ## Quick Reference
//!
//! **For CLI tools / subprocess servers:**
//! ```ignore
//! // Client spawning an MCP server
//! let transport = SpawnedTransport::spawn("my-mcp-server", &[]).await?;
//!
//! // Server reading from stdin/stdout
//! let transport = SyncStdioTransport::new();
//! ```
//!
//! **For HTTP (Streamable HTTP transport):**
//! ```ignore
//! // Client
//! let transport = HttpTransport::connect("http://localhost:8080/mcp").await?;
//!
//! // Server (requires `http` feature)
//! let listener = HttpTransportListener::bind("0.0.0.0:8080").await?;
//! ```
//!
//! **For WebSocket:**
//! ```ignore
//! // Client
//! let transport = WebSocketTransport::connect("ws://localhost:8080/mcp").await?;
//!
//! // Server
//! let listener = WebSocketListener::bind("0.0.0.0:8080").await?;
//! ```
//!
//! **For testing:**
//! ```ignore
//! let (client, server) = MemoryTransport::pair();
//! ```
//!
//! # Runtime Support
//!
//! This crate supports multiple async runtimes through feature flags:
//!
//! - `tokio-runtime` (default): Use Tokio for async I/O
//! - `smol-runtime`: Use smol for async I/O
//!
//! # Example
//!
//! ```no_run
//! use mcpkit_transport::{Transport, SpawnedTransport};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), mcpkit_transport::TransportError> {
//!     // Spawn an MCP server as a subprocess
//!     let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;
//!
//!     // Send and receive messages
//!     while let Some(msg) = transport.recv().await? {
//!         // Handle the message
//!     }
//!
//!     transport.close().await?;
//!     Ok(())
//! }
//! ```

#![deny(missing_docs)]

pub mod error;
pub mod http;
pub mod memory;
pub mod middleware;
pub mod pool;
pub mod runtime;
pub mod spawn;
pub mod stdio;
pub mod telemetry;
pub mod traits;
pub mod websocket;

#[cfg(feature = "grpc")]
pub mod grpc;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

// Re-export commonly used types
pub use error::TransportError;
pub use traits::{Transport, TransportExt, TransportListener, TransportMetadata};

// Re-export bytes types for zero-copy message handling
pub use bytes::{Bytes, BytesMut};

// Runtime-agnostic transports - available when ANY runtime is enabled
#[cfg(any(feature = "tokio-runtime", feature = "smol-runtime"))]
pub use memory::MemoryTransport;

// Note: StdioTransport has runtime-specific type parameters, so we re-export
// the module rather than a specific type alias
pub use stdio::SyncStdioTransport;

// HTTP transport (always export config/builder, listener only with http feature)
#[cfg(feature = "http")]
pub use http::HttpTransportListener;
pub use http::{HttpTransport, HttpTransportBuilder, HttpTransportConfig};

// WebSocket transport
pub use websocket::{
    ConnectionState, ExponentialBackoff, WebSocketConfig, WebSocketListener, WebSocketServerConfig,
    WebSocketTransport, WebSocketTransportBuilder,
};

// Unix socket transport
#[cfg(unix)]
pub use unix::{UnixListener, UnixSocketConfig, UnixTransport, UnixTransportBuilder};

// Windows named pipe transport
#[cfg(windows)]
pub use windows::{NamedPipeBuilder, NamedPipeConfig, NamedPipeServer, NamedPipeTransport};

// gRPC transport (requires `grpc` feature)
#[cfg(feature = "grpc")]
pub use grpc::{GrpcConfig, GrpcTransport};

// Connection pooling
pub use pool::{Pool, PoolConfig, PoolStats, PooledConnection};

// Subprocess spawning
#[cfg(feature = "tokio-runtime")]
pub use spawn::{SpawnedTransport, SpawnedTransportBuilder};

// Telemetry
pub use telemetry::{
    LatencyHistogram, MetricsSnapshot, TelemetryConfig, TelemetryLayer, TelemetryMetrics,
    TelemetryTransport,
};

// OpenTelemetry integration (requires `opentelemetry` feature)
#[cfg(feature = "opentelemetry")]
pub use telemetry::otel::{OtelConfig, TracingGuard, init_tracing, init_tracing_default};

// Prometheus metrics (requires `prometheus` feature)
#[cfg(feature = "prometheus")]
pub use telemetry::prom::{McpMetrics, MetricsExporter, create_default_metrics};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::error::TransportError;
    pub use crate::traits::{Transport, TransportExt, TransportListener, TransportMetadata};

    #[cfg(any(feature = "tokio-runtime", feature = "smol-runtime"))]
    pub use crate::memory::MemoryTransport;

    pub use crate::stdio::SyncStdioTransport;

    // HTTP
    pub use crate::http::{HttpTransport, HttpTransportConfig};

    // WebSocket
    pub use crate::websocket::{WebSocketConfig, WebSocketServerConfig, WebSocketTransport};

    // Unix
    #[cfg(unix)]
    pub use crate::unix::{UnixListener, UnixTransport};

    // Windows
    #[cfg(windows)]
    pub use crate::windows::{NamedPipeServer, NamedPipeTransport};

    // Pool
    pub use crate::pool::{Pool, PoolConfig, PooledConnection};

    // Subprocess spawning
    #[cfg(feature = "tokio-runtime")]
    pub use crate::spawn::{SpawnedTransport, SpawnedTransportBuilder};
}
