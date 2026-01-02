//! Retry policies for LLM provider operations.
//!
//! This module provides configurable retry logic with exponential backoff
//! for handling transient failures when communicating with LLM providers.

use std::time::Duration;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial delay between retries.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Whether to add jitter to delays.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config with no retries.
    #[must_use]
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Create an aggressive retry config for critical operations.
    #[must_use]
    pub fn aggressive() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }

    /// Set the maximum number of retries.
    #[must_use]
    pub const fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the initial delay.
    #[must_use]
    pub const fn initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay.
    #[must_use]
    pub const fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the backoff multiplier.
    #[must_use]
    pub const fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Enable or disable jitter.
    #[must_use]
    pub const fn jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Calculate the delay for a given attempt number (1-indexed).
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        // Calculate exponential backoff
        let base_delay = self.initial_delay.as_millis() as f64;
        let multiplier = self.backoff_multiplier.powi(attempt.saturating_sub(1) as i32);
        let delay_ms = base_delay * multiplier;

        // Apply max delay cap
        let delay_ms = delay_ms.min(self.max_delay.as_millis() as f64);

        // Apply jitter if enabled (±25%)
        let delay_ms = if self.jitter {
            let jitter_range = delay_ms * 0.25;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            (delay_ms + jitter).max(0.0)
        } else {
            delay_ms
        };

        Duration::from_millis(delay_ms as u64)
    }
}

/// Retry policy that can be used to determine retry behavior.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    config: RetryConfig,
    current_attempt: u32,
}

impl RetryPolicy {
    /// Create a new retry policy with the given config.
    #[must_use]
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            current_attempt: 0,
        }
    }

    /// Check if another retry should be attempted.
    #[must_use]
    pub fn should_retry(&self) -> bool {
        self.current_attempt < self.config.max_retries
    }

    /// Get the delay before the next retry.
    #[must_use]
    pub fn next_delay(&self) -> Duration {
        self.config.delay_for_attempt(self.current_attempt + 1)
    }

    /// Record a retry attempt.
    pub fn record_attempt(&mut self) {
        self.current_attempt = self.current_attempt.saturating_add(1);
    }

    /// Reset the policy for reuse.
    pub fn reset(&mut self) {
        self.current_attempt = 0;
    }

    /// Get the current attempt number.
    #[must_use]
    pub const fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    /// Get the remaining retry attempts.
    #[must_use]
    pub const fn remaining_attempts(&self) -> u32 {
        self.config.max_retries.saturating_sub(self.current_attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(1000));
    }

    #[test]
    fn test_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig::new()
            .initial_delay(Duration::from_millis(100))
            .backoff_multiplier(2.0)
            .jitter(false);

        // First attempt: 100ms
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100));
        // Second attempt: 200ms
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(200));
        // Third attempt: 400ms
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(400));
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig::new()
            .initial_delay(Duration::from_secs(10))
            .max_delay(Duration::from_secs(20))
            .backoff_multiplier(2.0)
            .jitter(false);

        // Should be capped at 20s
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(20));
    }

    #[test]
    fn test_retry_policy() {
        let config = RetryConfig::new().max_retries(2);
        let mut policy = RetryPolicy::new(config);

        assert!(policy.should_retry());
        assert_eq!(policy.remaining_attempts(), 2);

        policy.record_attempt();
        assert!(policy.should_retry());
        assert_eq!(policy.remaining_attempts(), 1);

        policy.record_attempt();
        assert!(!policy.should_retry());
        assert_eq!(policy.remaining_attempts(), 0);

        policy.reset();
        assert!(policy.should_retry());
        assert_eq!(policy.remaining_attempts(), 2);
    }
}
