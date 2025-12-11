//! Tower-compatible middleware for MCP transports.
//!
//! This module provides a middleware layer system compatible with Tower patterns,
//! allowing composable transport wrappers for logging, timeouts, retries, and metrics.
//!
//! # Design Philosophy
//!
//! Per [Tower Service Trait](https://docs.rs/tower-service/latest/tower_service/trait.Service.html),
//! this design is runtime-agnostic and provides composable, reusable middleware.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::middleware::{LayerStack, IdentityLayer};
//! use mcpkit_transport::{MemoryTransport, Transport};
//!
//! // Create a transport pair
//! let (client, _server) = MemoryTransport::pair();
//!
//! // Apply middleware layers
//! let stack = LayerStack::new(client)
//!     .with(IdentityLayer);  // No-op layer as example
//!
//! // Get the wrapped transport
//! let transport = stack.into_inner();
//! assert!(transport.is_connected());
//! ```

mod logging;
mod metrics;
mod rate_limit;
mod retry;
mod timeout;

pub use logging::LoggingLayer;
pub use metrics::MetricsLayer;
pub use rate_limit::{
    RateLimitLayer, RateLimitConfig, RateLimitAlgorithm, RateLimitAction,
    RateLimiter, RateLimitedTransport, RateLimitStats,
};
pub use retry::{RetryLayer, RetryPolicy, ExponentialBackoff};
pub use timeout::TimeoutLayer;

use crate::traits::Transport;

/// A layer that wraps a transport to add functionality.
///
/// This is inspired by Tower's `Layer` trait but adapted for our
/// transport abstraction. Layers transform transports into new transports
/// with additional behavior.
///
/// See [`IdentityLayer`] for a simple example implementation.
pub trait TransportLayer<T: Transport> {
    /// The transport type produced by this layer.
    type Transport: Transport;

    /// Wrap the inner transport with this layer's functionality.
    fn layer(&self, inner: T) -> Self::Transport;
}

/// A stack of layers applied to a transport.
///
/// `LayerStack` allows composing multiple layers together in a type-safe way.
/// Layers are applied from left to right (first added, innermost).
pub struct LayerStack<T> {
    inner: T,
}

impl<T: Transport> LayerStack<T> {
    /// Create a new layer stack with the given transport.
    pub fn new(transport: T) -> Self {
        Self { inner: transport }
    }

    /// Apply a layer to the stack.
    ///
    /// The layer wraps the current transport, producing a new transport type.
    pub fn with<L>(self, layer: L) -> LayerStack<L::Transport>
    where
        L: TransportLayer<T>,
    {
        LayerStack {
            inner: layer.layer(self.inner),
        }
    }

    /// Get the inner transport.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Get a reference to the inner transport.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

/// Identity layer that passes through without modification.
///
/// Useful as a default or placeholder.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityLayer;

impl<T: Transport> TransportLayer<T> for IdentityLayer {
    type Transport = T;

    fn layer(&self, inner: T) -> Self::Transport {
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_layer() {
        // IdentityLayer should be a no-op
        let _layer = IdentityLayer;
    }
}
