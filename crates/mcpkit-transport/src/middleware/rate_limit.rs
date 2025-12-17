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
//! # Production Checklist
//!
//! Before deploying to production, ensure you have:
//!
//! 1. **Enabled rate limiting** on all public-facing transports
//! 2. **Tuned limits** based on your expected usage patterns
//! 3. **Configured alerts** for rate limit rejections (indicates attack or misconfiguration)
//! 4. **Tested behavior** under rate limiting conditions
//! 5. **Documented limits** for clients to understand expected behavior

use crate::error::TransportError;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Use async-lock for runtime-agnostic async mutex
use async_lock::Mutex;

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

    /// Calculate the refill rate for token bucket (tokens per millisecond).
    fn refill_rate(&self) -> f64 {
        self.max_requests as f64 / self.window.as_millis() as f64
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

/// Rate limiter state.
struct RateLimiterState {
    /// Token bucket: current token count (scaled by 1000 for precision).
    tokens: AtomicU64,
    /// Last refill time.
    last_refill: Mutex<Instant>,
    /// Sliding window: request timestamps.
    request_times: Mutex<Vec<Instant>>,
    /// Fixed window: request count in current window.
    window_count: AtomicU64,
    /// Fixed window: window start time.
    window_start: Mutex<Instant>,
    /// Total requests tracked (for metrics).
    total_requests: AtomicU64,
    /// Total rejected requests (for metrics).
    total_rejected: AtomicU64,
}

impl RateLimiterState {
    fn new(config: &RateLimitConfig) -> Self {
        Self {
            // Start with full bucket (scaled by 1000)
            tokens: AtomicU64::new(config.burst_size * 1000),
            last_refill: Mutex::new(Instant::now()),
            request_times: Mutex::new(Vec::with_capacity(config.max_requests as usize)),
            window_count: AtomicU64::new(0),
            window_start: Mutex::new(Instant::now()),
            total_requests: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
        }
    }
}

/// Rate limiter that can be shared across transports.
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    state: Arc<RateLimiterState>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration.
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            state: Arc::new(RateLimiterState::new(&config)),
            config,
        }
    }

    /// Check if a request is allowed and consume a token if so.
    ///
    /// Returns `Ok(())` if allowed, `Err(TransportError)` if rate limited.
    pub async fn check(&self) -> Result<(), TransportError> {
        self.state.total_requests.fetch_add(1, Ordering::Relaxed);

        let allowed = match self.config.algorithm {
            RateLimitAlgorithm::TokenBucket => self.check_token_bucket().await,
            RateLimitAlgorithm::SlidingWindow => self.check_sliding_window().await,
            RateLimitAlgorithm::FixedWindow => self.check_fixed_window().await,
        };

        if allowed {
            Ok(())
        } else {
            self.state.total_rejected.fetch_add(1, Ordering::Relaxed);

            match self.config.on_limit {
                RateLimitAction::Reject => Err(TransportError::RateLimited {
                    retry_after: Some(self.config.window),
                }),
                RateLimitAction::Wait => {
                    // Wait for token refill and retry
                    let wait_time =
                        Duration::from_millis((1000.0 / self.config.refill_rate()).max(1.0) as u64);
                    crate::runtime::sleep(wait_time).await;
                    // Retry after waiting
                    Box::pin(self.check()).await
                }
                RateLimitAction::WarnAndAllow => {
                    tracing::warn!(
                        "Rate limit exceeded but allowing request (warn_and_allow mode)"
                    );
                    Ok(())
                }
            }
        }
    }

    /// Token bucket algorithm implementation.
    async fn check_token_bucket(&self) -> bool {
        let now = Instant::now();

        // Refill tokens based on elapsed time
        let mut last_refill = self.state.last_refill.lock().await;

        let elapsed = now.duration_since(*last_refill);
        let tokens_to_add =
            (elapsed.as_millis() as f64 * self.config.refill_rate() * 1000.0) as u64;

        if tokens_to_add > 0 {
            let current = self.state.tokens.load(Ordering::Relaxed);
            let max_tokens = self.config.burst_size * 1000;
            let new_tokens = (current + tokens_to_add).min(max_tokens);
            self.state.tokens.store(new_tokens, Ordering::Relaxed);
            *last_refill = now;
        }

        // Try to consume a token (1000 units = 1 token)
        loop {
            let current = self.state.tokens.load(Ordering::Relaxed);
            if current < 1000 {
                return false;
            }
            match self.state.tokens.compare_exchange(
                current,
                current - 1000,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => {} // Retry - loop continues naturally
            }
        }
    }

    /// Sliding window algorithm implementation.
    async fn check_sliding_window(&self) -> bool {
        let now = Instant::now();
        // If window is larger than process uptime, keep all requests (they're all within the window)
        let window_start = now.checked_sub(self.config.window);

        let mut times = self.state.request_times.lock().await;

        // Remove requests outside the window
        // If window_start is None (window > process uptime), keep all requests
        times.retain(|&t| window_start.is_none_or(|start| t > start));

        // Check if we're under the limit
        if times.len() < self.config.max_requests as usize {
            times.push(now);
            true
        } else {
            false
        }
    }

    /// Fixed window algorithm implementation.
    async fn check_fixed_window(&self) -> bool {
        let now = Instant::now();

        let mut window_start = self.state.window_start.lock().await;

        // Check if we need to start a new window
        if now.duration_since(*window_start) >= self.config.window {
            *window_start = now;
            self.state.window_count.store(1, Ordering::Relaxed);
            return true;
        }

        // Increment count and check limit
        let count = self.state.window_count.fetch_add(1, Ordering::Relaxed) + 1;
        count <= self.config.max_requests
    }

    /// Get the current token count (for token bucket).
    #[must_use]
    pub fn tokens(&self) -> u64 {
        self.state.tokens.load(Ordering::Relaxed) / 1000
    }

    /// Get statistics about rate limiting.
    #[must_use]
    pub fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            total_requests: self.state.total_requests.load(Ordering::Relaxed),
            total_rejected: self.state.total_rejected.load(Ordering::Relaxed),
            current_tokens: self.tokens(),
        }
    }

    /// Reset the rate limiter state.
    pub async fn reset(&self) {
        self.state
            .tokens
            .store(self.config.burst_size * 1000, Ordering::Relaxed);
        self.state.window_count.store(0, Ordering::Relaxed);
        self.state.total_requests.store(0, Ordering::Relaxed);
        self.state.total_rejected.store(0, Ordering::Relaxed);

        *self.state.last_refill.lock().await = Instant::now();
        *self.state.window_start.lock().await = Instant::now();
        self.state.request_times.lock().await.clear();
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

    /// Get rate limiting statistics.
    #[must_use]
    pub fn stats(&self) -> RateLimitStats {
        self.limiter.stats()
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

        let stats = limiter.stats();
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
        limiter.reset().await;

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
}
