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
//! ```ignore
//! use mcp_transport::middleware::{LayerStack, LoggingLayer, TimeoutLayer};
//! use std::time::Duration;
//!
//! let transport = StdioTransport::new();
//! let transport = LayerStack::new(transport)
//!     .with(LoggingLayer::new(tracing::Level::DEBUG))
//!     .with(TimeoutLayer::new(Duration::from_secs(30)))
//!     .into_inner();
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
/// # Example
///
/// ```ignore
/// struct MyLayer { /* config */ }
///
/// impl<T: Transport> TransportLayer<T> for MyLayer {
///     type Transport = MyTransport<T>;
///
///     fn layer(&self, inner: T) -> Self::Transport {
///         MyTransport::new(inner)
///     }
/// }
/// ```
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
///
/// # Example
///
/// ```ignore
/// let stack = LayerStack::new(transport)
///     .with(logging)    // Applied first (innermost)
///     .with(timeout)    // Applied second
///     .with(retry);     // Applied last (outermost)
/// ```
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
