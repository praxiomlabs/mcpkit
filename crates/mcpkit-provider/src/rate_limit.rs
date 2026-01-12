//! Rate limiting for LLM provider operations.
//!
//! This module provides token bucket rate limiting to prevent exceeding
//! provider rate limits and manage request throughput.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Configuration for rate limiting.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute.
    pub requests_per_minute: u32,
    /// Maximum tokens per minute.
    pub tokens_per_minute: u32,
    /// Burst capacity (how many requests can be made immediately).
    pub burst_capacity: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            tokens_per_minute: 100_000,
            burst_capacity: 10,
        }
    }
}

impl RateLimitConfig {
    /// Create a new rate limit config.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set requests per minute.
    #[must_use]
    pub const fn requests_per_minute(mut self, rpm: u32) -> Self {
        self.requests_per_minute = rpm;
        self
    }

    /// Set tokens per minute.
    #[must_use]
    pub const fn tokens_per_minute(mut self, tpm: u32) -> Self {
        self.tokens_per_minute = tpm;
        self
    }

    /// Set burst capacity.
    #[must_use]
    pub const fn burst_capacity(mut self, capacity: u32) -> Self {
        self.burst_capacity = capacity;
        self
    }

    /// Create a config for `OpenAI`'s free tier.
    #[must_use]
    pub fn openai_free() -> Self {
        Self {
            requests_per_minute: 3,
            tokens_per_minute: 40_000,
            burst_capacity: 1,
        }
    }

    /// Create a config for `OpenAI`'s tier 1.
    #[must_use]
    pub fn openai_tier1() -> Self {
        Self {
            requests_per_minute: 500,
            tokens_per_minute: 200_000,
            burst_capacity: 50,
        }
    }

    /// Create a config for Anthropic's default limits.
    #[must_use]
    pub fn anthropic_default() -> Self {
        Self {
            requests_per_minute: 50,
            tokens_per_minute: 100_000,
            burst_capacity: 10,
        }
    }
}

/// A token bucket rate limiter.
///
/// This implements the token bucket algorithm, where tokens are added at a
/// constant rate and consumed by requests. Requests are delayed if there
/// aren't enough tokens available.
pub struct RateLimiter {
    config: RateLimitConfig,
    tokens: AtomicU64,
    last_refill: Mutex<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given config.
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            tokens: AtomicU64::new(u64::from(config.burst_capacity)),
            last_refill: Mutex::new(Instant::now()),
            config,
        }
    }

    /// Acquire a permit to make a request.
    ///
    /// This will block until a token is available.
    pub async fn acquire(&self) {
        self.acquire_n(1).await;
    }

    /// Acquire multiple permits.
    ///
    /// This will block until all tokens are available.
    pub async fn acquire_n(&self, n: u32) {
        loop {
            // Refill tokens based on elapsed time
            self.refill().await;

            // Try to acquire tokens
            let current = self.tokens.load(Ordering::Acquire);
            let needed = u64::from(n);

            if current >= needed {
                if self
                    .tokens
                    .compare_exchange(
                        current,
                        current - needed,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
                // CAS failed, retry
                continue;
            }

            // Not enough tokens, wait for refill
            let tokens_needed = needed - current;
            let wait_time = self.time_for_tokens(tokens_needed);
            tokio::time::sleep(wait_time).await;
        }
    }

    /// Try to acquire a permit without blocking.
    ///
    /// Returns `true` if the permit was acquired, `false` otherwise.
    pub async fn try_acquire(&self) -> bool {
        self.try_acquire_n(1).await
    }

    /// Try to acquire multiple permits without blocking.
    pub async fn try_acquire_n(&self, n: u32) -> bool {
        self.refill().await;

        let current = self.tokens.load(Ordering::Acquire);
        let needed = u64::from(n);

        if current >= needed {
            self.tokens
                .compare_exchange(
                    current,
                    current - needed,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
        } else {
            false
        }
    }

    /// Get the current number of available tokens.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }

    /// Refill tokens based on elapsed time.
    async fn refill(&self) {
        let mut last_refill = self.last_refill.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);

        // Calculate tokens to add based on elapsed time
        let tokens_per_ms = f64::from(self.config.requests_per_minute) / 60_000.0;
        let tokens_to_add = (elapsed.as_millis() as f64 * tokens_per_ms) as u64;

        if tokens_to_add > 0 {
            let current = self.tokens.load(Ordering::Acquire);
            let new_tokens = (current + tokens_to_add).min(u64::from(self.config.burst_capacity));
            self.tokens.store(new_tokens, Ordering::Release);
            *last_refill = now;
        }
    }

    /// Calculate how long to wait for a given number of tokens.
    fn time_for_tokens(&self, tokens: u64) -> Duration {
        let tokens_per_ms = f64::from(self.config.requests_per_minute) / 60_000.0;
        let ms_needed = tokens as f64 / tokens_per_ms;
        Duration::from_millis(ms_needed.ceil() as u64)
    }
}

/// A rate limiter that can be shared across tasks.
pub type SharedRateLimiter = Arc<RateLimiter>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_minute, 60);
        assert_eq!(config.tokens_per_minute, 100_000);
    }

    #[test]
    fn test_openai_configs() {
        let free = RateLimitConfig::openai_free();
        assert_eq!(free.requests_per_minute, 3);

        let tier1 = RateLimitConfig::openai_tier1();
        assert_eq!(tier1.requests_per_minute, 500);
    }

    #[tokio::test]
    async fn test_try_acquire() {
        let config = RateLimitConfig::new().burst_capacity(2);
        let limiter = RateLimiter::new(config);

        // Should succeed twice
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);

        // Should fail (no tokens left)
        assert!(!limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn test_available() {
        let config = RateLimitConfig::new().burst_capacity(5);
        let limiter = RateLimiter::new(config);

        assert_eq!(limiter.available(), 5);

        limiter.acquire_n(3).await;
        assert_eq!(limiter.available(), 2);
    }
}
