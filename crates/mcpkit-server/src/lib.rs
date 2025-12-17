//! Server implementation for the MCP SDK.
//!
//! This crate provides the server-side implementation for the Model Context
//! Protocol. It includes composable handler traits, a fluent builder API,
//! and request routing.
//!
//! # Overview
//!
//! Building an MCP server involves:
//!
//! 1. Implementing the [`ServerHandler`] trait (required)
//! 2. Implementing optional capability traits ([`ToolHandler`], [`ResourceHandler`], etc.)
//! 3. Using [`ServerBuilder`] to create a configured server
//! 4. Running the server with a transport
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::{ServerBuilder, ServerHandler};
//! use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
//!
//! struct MyServer;
//!
//! impl ServerHandler for MyServer {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo::new("my-server", "1.0.0")
//!     }
//!
//!     fn capabilities(&self) -> ServerCapabilities {
//!         ServerCapabilities::new().with_tools()
//!     }
//! }
//!
//! let server = ServerBuilder::new(MyServer).build();
//! assert!(server.capabilities().has_tools());
//! ```
//!
//! # Handler Traits
//!
//! The server uses composable handler traits:
//!
//! - [`ServerHandler`]: Core trait required for all servers
//! - [`ToolHandler`]: Handle tool discovery and execution
//! - [`ResourceHandler`]: Handle resource discovery and reading
//! - [`PromptHandler`]: Handle prompt discovery and rendering
//! - [`TaskHandler`]: Handle long-running task operations
//! - [`SamplingHandler`]: Handle server-initiated LLM requests
//! - [`ElicitationHandler`]: Handle structured user input requests
//!
//! # Context
//!
//! Handlers receive a [`Context`] that provides:
//!
//! - Request metadata (ID, progress token)
//! - Client and server capabilities
//! - Cancellation checking
//! - Progress reporting
//! - Notification sending

#![deny(missing_docs)]

pub mod builder;
pub mod capability;
pub mod context;
pub mod handler;
pub mod metrics;
pub mod router;
pub mod server;
pub mod state;

// Re-export commonly used types
pub use builder::{FullServer, MinimalServer, NotRegistered, Registered, Server, ServerBuilder};
pub use context::{CancellationToken, CancelledFuture, Context, ContextData, NoOpPeer, Peer};
pub use handler::{
    CompletionHandler, ElicitationHandler, LogLevel, LoggingHandler, PromptHandler,
    ResourceHandler, SamplingHandler, ServerHandler, TaskHandler, ToolHandler,
};
pub use metrics::{MethodStats, MetricsSnapshot, ServerMetrics};
pub use router::{route_prompts, route_resources, route_tools};
pub use server::{RequestRouter, RuntimeConfig, ServerRuntime, ServerState, TransportPeer};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::builder::{
        FullServer, MinimalServer, NotRegistered, Registered, Server, ServerBuilder,
    };
    pub use crate::context::{
        CancellationToken, CancelledFuture, Context, ContextData, NoOpPeer, Peer,
    };
    pub use crate::handler::{
        CompletionHandler, ElicitationHandler, LogLevel, LoggingHandler, PromptHandler,
        ResourceHandler, SamplingHandler, ServerHandler, TaskHandler, ToolHandler,
    };
    pub use crate::metrics::{MethodStats, MetricsSnapshot, ServerMetrics};
}
