//! Rate limiting middleware for MCP transports.
//!
//! This module provides rate limiting functionality to prevent abuse and
//! ensure fair resource usage across clients.
//!
//! # Security Warning
//!
//! **Rate limiting is NOT enabled by default.** You must explicitly configure
//! and apply rate limiting middleware to your transport to protect against
//! denial-of-service attacks.
//!
//! Without rate limiting, a malicious or misbehaving client can:
//! - Exhaust server resources with excessive requests
//! - Cause service degradation for other clients
//! - Trigger expensive operations repeatedly
//!
//! # Recommended Configuration
//!
//! For production deployments, consider these guidelines:
//!
//! - **Tool calls**: Limit based on tool complexity (expensive tools = lower limits)
//! - **Resource reads**: Higher limits acceptable for cached/cheap resources
//! - **Burst handling**: Allow small bursts for legitimate interactive usage
//!
//! # Algorithms
//!
//! - **Token Bucket**: Allows bursts up to bucket size, then limits to rate
//! - **Sliding Window**: Tracks requests in a rolling time window
//! - **Fixed Window**: Simple per-window counting (least memory, least accurate)
//!
//! # Pluggable Storage Backends
//!
//! Rate limiting state can be stored in different backends using the
//! [`RateLimitStore`] trait:
//!
//! - [`InMemoryStore`]: Default in-memory store (single-process deployments)
//! - Custom stores: Implement [`RateLimitStore`] for Redis, DynamoDB, etc.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::middleware::RateLimitConfig;
//! use std::time::Duration;
//!
//! // Configure rate limiting: 100 requests per minute with burst of 10
//! let config = RateLimitConfig::new(100, Duration::from_secs(60))
//!     .with_burst(10);
//!
//! assert_eq!(config.max_requests, 100);
//! assert_eq!(config.burst_size, 10);
//! ```
//!
//! # Custom Store Example
//!
//! ```rust,ignore
//! use mcpkit_transport::middleware::rate_limit::{
//!     RateLimitStore, RateLimitDecision, RateLimitStoreError, RateLimitConfig,
//! };
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! struct RedisStore { /* ... */ }
//!
//! #[async_trait]
//! impl RateLimitStore for RedisStore {
//!     async fn check_and_consume(
//!         &self,
//!         key: &str,
//!         config: &RateLimitConfig,
//!     ) -> Result<RateLimitDecision, RateLimitStoreError> {
//!         // Implement using Redis MULTI/EXEC or Lua scripts
//!         todo!()
//!     }
//!
//!     async fn get_stats(&self, key: &str) -> Result<StoreStats, RateLimitStoreError> {
//!         todo!()
//!     }
//!
//!     async fn reset(&self, key: &str) -> Result<(), RateLimitStoreError> {
//!         todo!()
//!     }
//! }
//!
//! // Use custom store with RateLimiter
//! let config = RateLimitConfig::new(100, Duration::from_secs(60));
//! let store = Arc::new(RedisStore { /* ... */ });
//! let limiter = RateLimiter::with_store(config, store);
//! ```
//!
//! # Production Checklist
//!
//! Before deploying to production, ensure you have:
//!
//! 1. **Enabled rate limiting** on all public-facing transports
//! 2. **Tuned limits** based on your expected usage patterns
//! 3. **Configured alerts** for rate limit rejections (indicates attack or misconfiguration)
//! 4. **Tested behavior** under rate limiting conditions
//! 5. **Documented limits** for clients to understand expected behavior
//! 6. **Considered distributed stores** for multi-instance deployments

mod store;

pub use store::{
    BoxedRateLimitStore, InMemoryStore, RateLimitDecision, RateLimitStore, RateLimitStoreError,
    StoreStats,
};

use crate::error::TransportError;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::Arc;
use std::time::Duration;

/// Rate limiting configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed in the window.
    pub max_requests: u64,
    /// The time window for rate limiting.
    pub window: Duration,
    /// Maximum burst size (for token bucket algorithm).
    pub burst_size: u64,
    /// The rate limiting algorithm to use.
    pub algorithm: RateLimitAlgorithm,
    /// Action to take when rate limited.
    pub on_limit: RateLimitAction,
}

impl RateLimitConfig {
    /// Create a new rate limit configuration.
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed per window
    /// * `window` - The time window for rate limiting
    #[must_use]
    pub const fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            burst_size: max_requests,
            algorithm: RateLimitAlgorithm::TokenBucket,
            on_limit: RateLimitAction::Reject,
        }
    }

    /// Set the burst size for token bucket algorithm.
    #[must_use]
    pub const fn with_burst(mut self, burst_size: u64) -> Self {
        self.burst_size = burst_size;
        self
    }

    /// Set the rate limiting algorithm.
    #[must_use]
    pub const fn with_algorithm(mut self, algorithm: RateLimitAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set the action to take when rate limited.
    #[must_use]
    pub const fn with_action(mut self, action: RateLimitAction) -> Self {
        self.on_limit = action;
        self
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        // Default: 100 requests per minute
        Self::new(100, Duration::from_secs(60))
    }
}

/// Rate limiting algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateLimitAlgorithm {
    /// Token bucket algorithm - allows bursts, smooth rate limiting.
    #[default]
    TokenBucket,
    /// Sliding window - tracks requests in a rolling time window.
    SlidingWindow,
    /// Fixed window - simple counter reset at window boundaries.
    FixedWindow,
}

/// Action to take when rate limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateLimitAction {
    /// Reject the request with an error.
    #[default]
    Reject,
    /// Wait until rate limit allows (with optional max wait time).
    Wait,
    /// Log a warning but allow the request.
    WarnAndAllow,
}

/// Rate limiter that can be shared across transports.
///
/// Uses a pluggable [`RateLimitStore`] for state storage, defaulting to
/// [`InMemoryStore`] for single-process deployments.
///
/// # Thread Safety
///
/// `RateLimiter` is `Clone` and can be safely shared across tasks.
/// The underlying store is accessed through `Arc<dyn RateLimitStore>`.
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    store: Arc<dyn RateLimitStore>,
    /// Key used for rate limiting (empty string for global rate limiting).
    key: String,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration.
    ///
    /// Uses the default [`InMemoryStore`] for state storage.
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        let store = Arc::new(InMemoryStore::new(&config));
        Self {
            config,
            store,
            key: String::new(),
        }
    }

    /// Create a rate limiter with a custom store backend.
    ///
    /// Use this for distributed rate limiting with Redis, DynamoDB, etc.
    ///
    /// # Arguments
    ///
    /// * `config` - Rate limit configuration
    /// * `store` - Custom store implementation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use mcpkit_transport::middleware::rate_limit::{RateLimiter, RateLimitConfig};
    /// use std::sync::Arc;
    ///
    /// let config = RateLimitConfig::new(100, Duration::from_secs(60));
    /// let redis_store = Arc::new(MyRedisStore::new(/* ... */));
    /// let limiter = RateLimiter::with_store(config, redis_store);
    /// ```
    #[must_use]
    pub fn with_store<S: RateLimitStore + 'static>(config: RateLimitConfig, store: Arc<S>) -> Self {
        Self {
            config,
            store,
            key: String::new(),
        }
    }

    /// Set the rate limit key for per-client rate limiting.
    ///
    /// The key is used to identify distinct rate limit buckets.
    /// Common choices include client ID, IP address, or API key.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Check if a request is allowed and consume a token if so.
    ///
    /// Returns `Ok(())` if allowed, `Err(TransportError)` if rate limited.
    pub async fn check(&self) -> Result<(), TransportError> {
        let decision = self
            .store
            .check_and_consume(&self.key, &self.config)
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("rate limit store error: {e}"),
            })?;

        match decision {
            RateLimitDecision::Allowed { .. } => Ok(()),
            RateLimitDecision::Denied { retry_after } => match self.config.on_limit {
                RateLimitAction::Reject => Err(TransportError::RateLimited {
                    retry_after: Some(retry_after),
                }),
                RateLimitAction::Wait => {
                    // Wait for the suggested retry_after duration and retry
                    crate::runtime::sleep(retry_after).await;
                    // Retry after waiting
                    Box::pin(self.check()).await
                }
                RateLimitAction::WarnAndAllow => {
                    tracing::warn!(
                        "Rate limit exceeded but allowing request (warn_and_allow mode)"
                    );
                    Ok(())
                }
            },
        }
    }

    /// Get the current token count (for token bucket).
    ///
    /// Note: This may not reflect the exact current state in distributed stores.
    #[must_use]
    pub async fn tokens(&self) -> u64 {
        self.store
            .get_stats(&self.key)
            .await
            .map_or(0, |s| s.current_tokens)
    }

    /// Get statistics about rate limiting.
    #[must_use]
    pub async fn stats(&self) -> RateLimitStats {
        match self.store.get_stats(&self.key).await {
            Ok(store_stats) => RateLimitStats {
                total_requests: store_stats.total_requests,
                total_rejected: store_stats.total_rejected,
                current_tokens: store_stats.current_tokens,
            },
            Err(_) => RateLimitStats {
                total_requests: 0,
                total_rejected: 0,
                current_tokens: 0,
            },
        }
    }

    /// Reset the rate limiter state.
    pub async fn reset(&self) -> Result<(), TransportError> {
        self.store
            .reset(&self.key)
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("rate limit store error: {e}"),
            })
    }

    /// Get the configuration.
    #[must_use]
    pub const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Get the store (for testing or advanced use cases).
    #[must_use]
    pub fn store(&self) -> &Arc<dyn RateLimitStore> {
        &self.store
    }
}

/// Rate limiting statistics.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitStats {
    /// Total requests received.
    pub total_requests: u64,
    /// Total requests rejected.
    pub total_rejected: u64,
    /// Current token count (for token bucket).
    pub current_tokens: u64,
}

impl RateLimitStats {
    /// Calculate the rejection rate.
    #[must_use]
    pub fn rejection_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_rejected as f64 / self.total_requests as f64
        }
    }
}

/// Log a warning about rate limiting for production deployments.
///
/// Call this during server initialization if rate limiting is not configured
/// to remind operators about the importance of rate limiting.
///
/// # Example
///
/// ```rust
/// use mcpkit_transport::middleware::log_rate_limit_warning;
///
/// // Call during server startup if rate limiting is not applied
/// log_rate_limit_warning();
/// ```
pub fn log_rate_limit_warning() {
    tracing::warn!(
        target: "mcpkit::security",
        "⚠️  SECURITY WARNING: Rate limiting is NOT enabled. \
         Without rate limiting, your MCP server is vulnerable to: \
         resource exhaustion, denial of service, and expensive operation abuse. \
         Configure rate limiting with RateLimitLayer for production deployments."
    );
}

/// Rate limiting layer for transports.
pub struct RateLimitLayer {
    limiter: RateLimiter,
}

impl RateLimitLayer {
    /// Create a new rate limit layer.
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            limiter: RateLimiter::new(config),
        }
    }

    /// Create with a shared rate limiter.
    #[must_use]
    pub const fn with_limiter(limiter: RateLimiter) -> Self {
        Self { limiter }
    }

    /// Get the rate limiter.
    #[must_use]
    pub const fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }
}

impl<T: Transport> super::TransportLayer<T> for RateLimitLayer {
    type Transport = RateLimitedTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        RateLimitedTransport {
            inner,
            limiter: self.limiter.clone(),
        }
    }
}

/// A transport wrapped with rate limiting.
pub struct RateLimitedTransport<T> {
    inner: T,
    limiter: RateLimiter,
}

impl<T: Transport> RateLimitedTransport<T> {
    /// Create a new rate limited transport.
    #[must_use]
    pub fn new(inner: T, config: RateLimitConfig) -> Self {
        Self {
            inner,
            limiter: RateLimiter::new(config),
        }
    }

    /// Create with a custom store backend.
    #[must_use]
    pub fn with_store<S: RateLimitStore + 'static>(
        inner: T,
        config: RateLimitConfig,
        store: Arc<S>,
    ) -> Self {
        Self {
            inner,
            limiter: RateLimiter::with_store(config, store),
        }
    }

    /// Get rate limiting statistics.
    pub async fn stats(&self) -> RateLimitStats {
        self.limiter.stats().await
    }

    /// Get the inner transport.
    pub const fn inner(&self) -> &T {
        &self.inner
    }

    /// Get the rate limiter.
    pub const fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }
}

impl<T: Transport> Transport for RateLimitedTransport<T> {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        // Rate limit outbound messages
        self.limiter.check().await?;
        self.inner
            .send(msg)
            .await
            .map_err(|e| TransportError::Connection {
                message: e.to_string(),
            })
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        // Note: We don't rate limit incoming messages by default
        // as that's controlled by the sender
        self.inner
            .recv()
            .await
            .map_err(|e| TransportError::Connection {
                message: e.to_string(),
            })
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.inner
            .close()
            .await
            .map_err(|e| TransportError::Connection {
                message: e.to_string(),
            })
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn metadata(&self) -> TransportMetadata {
        self.inner.metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config() {
        let config = RateLimitConfig::new(100, Duration::from_secs(60))
            .with_burst(10)
            .with_algorithm(RateLimitAlgorithm::TokenBucket)
            .with_action(RateLimitAction::Reject);

        assert_eq!(config.max_requests, 100);
        assert_eq!(config.burst_size, 10);
        assert_eq!(config.algorithm, RateLimitAlgorithm::TokenBucket);
        assert_eq!(config.on_limit, RateLimitAction::Reject);
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_requests() {
        let config = RateLimitConfig::new(10, Duration::from_secs(1));
        let limiter = RateLimiter::new(config);

        // Should allow first 10 requests
        for _ in 0..10 {
            assert!(limiter.check().await.is_ok());
        }

        // 11th request should be rate limited
        assert!(limiter.check().await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_stats() {
        let config = RateLimitConfig::new(5, Duration::from_secs(1));
        let limiter = RateLimiter::new(config);

        // Make some requests
        for _ in 0..7 {
            let _ = limiter.check().await;
        }

        let stats = limiter.stats().await;
        assert_eq!(stats.total_requests, 7);
        assert_eq!(stats.total_rejected, 2); // Last 2 were rejected
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let config = RateLimitConfig::new(5, Duration::from_secs(1));
        let limiter = RateLimiter::new(config);

        // Exhaust all tokens
        for _ in 0..5 {
            assert!(limiter.check().await.is_ok());
        }
        assert!(limiter.check().await.is_err());

        // Reset
        limiter.reset().await.unwrap();

        // Should allow requests again
        assert!(limiter.check().await.is_ok());
    }

    #[tokio::test]
    async fn test_sliding_window_algorithm() {
        let config = RateLimitConfig::new(5, Duration::from_millis(100))
            .with_algorithm(RateLimitAlgorithm::SlidingWindow);
        let limiter = RateLimiter::new(config);

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check().await.is_ok());
        }

        // 6th should be rejected
        assert!(limiter.check().await.is_err());

        // Wait for window to pass
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow requests again
        assert!(limiter.check().await.is_ok());
    }

    #[tokio::test]
    async fn test_fixed_window_algorithm() {
        let config = RateLimitConfig::new(5, Duration::from_millis(100))
            .with_algorithm(RateLimitAlgorithm::FixedWindow);
        let limiter = RateLimiter::new(config);

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check().await.is_ok());
        }

        // 6th should be rejected
        assert!(limiter.check().await.is_err());

        // Wait for window to reset
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow requests again
        assert!(limiter.check().await.is_ok());
    }

    #[test]
    fn test_rejection_rate() {
        let stats = RateLimitStats {
            total_requests: 100,
            total_rejected: 25,
            current_tokens: 10,
        };

        assert!((stats.rejection_rate() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rejection_rate_zero_requests() {
        let stats = RateLimitStats {
            total_requests: 0,
            total_rejected: 0,
            current_tokens: 10,
        };

        assert!((stats.rejection_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_with_custom_store() {
        // Test that custom stores can be injected
        let config = RateLimitConfig::new(10, Duration::from_secs(1));
        let store = Arc::new(InMemoryStore::new(&config));

        // Create limiter with custom store
        let limiter = RateLimiter::with_store(config, store.clone());

        // Should work the same as default
        assert!(limiter.check().await.is_ok());

        // Stats should be accessible through the store
        let stats = store.get_stats("").await.unwrap();
        assert_eq!(stats.total_requests, 1);
    }

    #[tokio::test]
    async fn test_rate_limiter_with_key() {
        let config = RateLimitConfig::new(5, Duration::from_secs(1));
        let store = Arc::new(InMemoryStore::new(&config));

        // Create two limiters with different keys (same store)
        let limiter1 = RateLimiter::with_store(config.clone(), store.clone()).with_key("client-1");
        let limiter2 = RateLimiter::with_store(config, store).with_key("client-2");

        // Both should be allowed (InMemoryStore uses global bucket, but API supports keys)
        assert!(limiter1.check().await.is_ok());
        assert!(limiter2.check().await.is_ok());
    }
}
