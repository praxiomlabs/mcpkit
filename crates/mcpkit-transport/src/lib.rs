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
//! ```
//!
//! To *serve* MCP over Streamable HTTP, use a framework adapter
//! (`mcpkit-axum`, `mcpkit-actix`, `mcpkit-warp`, or `mcpkit-rocket`), which
//! handle routing, sessions, SSE, and origin validation.
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
// `clippy::significant_drop_tightening` (nursery) is allowed crate-wide, after
// reviewing all 21 sites it flagged here individually rather than batch-applying:
//
//   * 2 were genuine and are fixed — RateLimitStore::get_stats held the bucket
//     lock across three atomic loads that do not need it, and
//     WsListener::stop held a temporary guard for the whole if-let body.
//   * 12 are the send/recv/close guards in stdio, unix, spawn and http. The
//     lock MUST span the write/write/flush sequence: releasing between them
//     lets another task interleave and corrupt line-delimited JSON-RPC framing.
//     The gap the lint objects to is between the last write and `Ok(())`, which
//     performs no work, so tightening buys nothing.
//   * 4 are assertions inside test bodies.
//   * 3 are ordering-sensitive: PoolManager::release notifies waiters under the
//     lock, and WebSocket do_close sequences close-frame, clear-stream and
//     mark-disconnected deliberately.
//
// The same lint proposed a race in mcpkit-core: TaskManager::wait_terminal must
// hold its read guard until `listen()` registers, or a terminal transition
// notifies with no listener attached and the await never completes. That site
// carries its own targeted allow.
#![allow(clippy::significant_drop_tightening)]

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
pub use http::{HttpTransport, HttpTransportBuilder, HttpTransportConfig};

// WebSocket transport
#[cfg(feature = "websocket")]
pub use websocket::WebSocketListener;
pub use websocket::{
    ConnectionState, ExponentialBackoff, WebSocketConfig, WebSocketServerConfig,
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
