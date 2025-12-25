//! gRPC transport for MCP communication.
//!
//! This module provides a gRPC-based transport for the Model Context Protocol,
//! enabling high-performance, strongly-typed communication between MCP clients
//! and servers.
//!
//! # Overview
//!
//! The gRPC transport wraps MCP JSON-RPC messages in a simple protobuf envelope,
//! allowing them to be transmitted efficiently over HTTP/2 with full streaming
//! support.
//!
//! # Features
//!
//! - **Bidirectional streaming**: Full-duplex communication using gRPC streams
//! - **HTTP/2**: Modern transport with multiplexing and header compression
//! - **TLS support**: Secure communication out of the box
//! - **Load balancing**: Compatible with gRPC load balancers (Envoy, etc.)
//!
//! # Current Status
//!
//! The gRPC transport is partially implemented:
//!
//! - **Client transport**: Fully functional - can connect to gRPC servers
//! - **Server transport**: TCP listener and service infrastructure are in place,
//!   but full bidirectional streaming integration with tonic requires additional
//!   work (protobuf codegen or manual service implementation)
//!
//! For production use, consider:
//! - Using the HTTP or WebSocket transports which are fully implemented
//! - Contributing the remaining gRPC server streaming implementation
//!
//! # Example
//!
//! ```ignore
//! use mcpkit_transport::grpc::{GrpcTransport, GrpcConfig};
//!
//! // Create a client transport
//! let config = GrpcConfig::new("http://localhost:50051");
//! let transport = GrpcTransport::connect(config).await?;
//!
//! // Use the transport with an MCP server
//! let server = handler.into_server();
//! server.serve(transport).await?;
//! ```
//!
//! # Protocol
//!
//! Messages are serialized as JSON and wrapped in a simple protobuf message:
//!
//! ```protobuf
//! message McpMessage {
//!     string payload = 1;  // JSON-RPC message as string
//!     map<string, string> metadata = 2;  // Optional metadata
//! }
//!
//! service McpService {
//!     rpc Stream(stream McpMessage) returns (stream McpMessage);
//! }
//! ```

mod transport;

pub use transport::{
    GrpcConfig, GrpcError, GrpcServer, GrpcServerBuilder, GrpcServerConfig, GrpcTransport,
    McpMessage,
};

/// Re-export tonic types for convenience.
pub use tonic;
