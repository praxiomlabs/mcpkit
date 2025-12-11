//! Retry middleware for MCP transports.
//!
//! This middleware adds automatic retry logic with configurable backoff
//! for transient failures.

use crate::error::TransportError;
use crate::middleware::TransportLayer;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for exponential backoff.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    /// Initial delay before first retry.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Multiplier applied after each retry.
    pub multiplier: f64,
    /// Optional jitter to add randomness to delays.
    pub jitter: Option<f64>,
}

impl ExponentialBackoff {
    /// Create a new exponential backoff configuration.
    #[must_use]
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial_delay: initial,
            max_delay: max,
            multiplier: 2.0,
            jitter: Some(0.1),
        }
    }

    /// Set the multiplier.
    #[must_use]
    pub fn multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Set jitter factor (0.0 to 1.0).
    #[must_use]
    pub fn jitter(mut self, jitter: f64) -> Self {
        self.jitter = Some(jitter.clamp(0.0, 1.0));
        self
    }

    /// Disable jitter.
    #[must_use]
    pub fn no_jitter(mut self) -> Self {
        self.jitter = None;
        self
    }

    /// Calculate delay for attempt number (0-indexed).
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.initial_delay.as_secs_f64() * self.multiplier.powi(attempt as i32);
        let delay = base.min(self.max_delay.as_secs_f64());

        let delay = if let Some(jitter) = self.jitter {
            // Add jitter: delay * (1 - jitter + 2*jitter*random)
            // Since we don't have random access here, we use a simple deterministic jitter
            let jitter_factor = 1.0 - jitter + 2.0 * jitter * (attempt as f64 % 1.0).fract();
            delay * jitter_factor
        } else {
            delay
        };

        Duration::from_secs_f64(delay)
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new(Duration::from_millis(100), Duration::from_secs(30))
    }
}

/// Policy for determining which errors should be retried.
pub trait RetryPolicy: Send + Sync {
    /// Check if the given error should be retried.
    fn should_retry(&self, error: &TransportError) -> bool;

    /// Clone this policy (object-safe clone).
    fn clone_box(&self) -> Box<dyn RetryPolicy>;
}

/// Default retry policy that retries transient errors.
#[derive(Debug, Clone, Default)]
pub struct DefaultRetryPolicy;

impl RetryPolicy for DefaultRetryPolicy {
    fn should_retry(&self, error: &TransportError) -> bool {
        matches!(
            error,
            TransportError::Timeout { .. }
                | TransportError::IoError(_)
                | TransportError::Io { .. }
                | TransportError::ConnectionClosed
                | TransportError::Connection { .. }
        )
    }

    fn clone_box(&self) -> Box<dyn RetryPolicy> {
        Box::new(self.clone())
    }
}

/// A layer that adds retry logic to a transport.
#[derive(Debug, Clone)]
pub struct RetryLayer {
    /// Maximum number of retry attempts.
    max_attempts: u32,
    /// Backoff configuration.
    backoff: ExponentialBackoff,
}

impl RetryLayer {
    /// Create a new retry layer with the specified max attempts.
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            backoff: ExponentialBackoff::default(),
        }
    }

    /// Set the backoff configuration.
    #[must_use]
    pub fn backoff(mut self, backoff: ExponentialBackoff) -> Self {
        self.backoff = backoff;
        self
    }
}

impl Default for RetryLayer {
    fn default() -> Self {
        Self::new(3)
    }
}

impl<T: Transport> TransportLayer<T> for RetryLayer
where
    T: Clone,
    T::Error: Into<TransportError> + From<TransportError>,
{
    type Transport = RetryTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        RetryTransport {
            inner,
            max_attempts: self.max_attempts,
            backoff: self.backoff.clone(),
            policy: Box::new(DefaultRetryPolicy),
        }
    }
}

/// A transport wrapped with retry logic.
pub struct RetryTransport<T> {
    inner: T,
    max_attempts: u32,
    backoff: ExponentialBackoff,
    policy: Box<dyn RetryPolicy>,
}

impl<T: Clone> Clone for RetryTransport<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_attempts: self.max_attempts,
            backoff: self.backoff.clone(),
            policy: self.policy.clone_box(),
        }
    }
}

impl<T: Transport + Clone> RetryTransport<T> {
    /// Set a custom retry policy.
    pub fn with_policy<P: RetryPolicy + 'static>(mut self, policy: P) -> Self {
        self.policy = Box::new(policy);
        self
    }
}

impl<T: Transport + Clone> Transport for RetryTransport<T>
where
    T::Error: Into<TransportError> + From<TransportError>,
{
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let mut last_error: Option<T::Error> = None;

        for attempt in 0..self.max_attempts {
            match self.inner.send(msg.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let transport_err: TransportError = e.into();

                    if !self.policy.should_retry(&transport_err) {
                        debug!(attempt, "error is not retriable, giving up");
                        return Err(transport_err.into());
                    }

                    if attempt + 1 < self.max_attempts {
                        let delay = self.backoff.delay_for_attempt(attempt);
                        warn!(
                            attempt,
                            delay_ms = delay.as_millis(),
                            error = %transport_err,
                            "send failed, retrying"
                        );
                        crate::runtime::sleep(delay).await;
                    }

                    last_error = Some(transport_err.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            TransportError::Protocol {
                message: "retry exhausted with no error".to_string(),
            }
            .into()
        }))
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        // Receive operations are generally not retriable in the same way
        // because they depend on the peer sending data
        self.inner.recv().await
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.inner.close().await
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
    fn test_exponential_backoff() {
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .no_jitter();

        assert_eq!(backoff.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(backoff.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(backoff.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(backoff.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn test_exponential_backoff_max() {
        let backoff = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(5),
        )
        .no_jitter();

        // Should cap at max
        assert_eq!(backoff.delay_for_attempt(10), Duration::from_secs(5));
    }

    #[test]
    fn test_default_retry_policy() {
        let policy = DefaultRetryPolicy;

        assert!(policy.should_retry(&TransportError::Timeout {
            operation: "send".to_string(),
            duration: Duration::from_secs(1),
        }));

        assert!(policy.should_retry(&TransportError::ConnectionClosed));

        assert!(!policy.should_retry(&TransportError::NotConnected));

        assert!(!policy.should_retry(&TransportError::Protocol {
            message: "invalid".to_string(),
        }));
    }

    #[test]
    fn test_retry_layer_creation() {
        let layer = RetryLayer::new(5)
            .backoff(ExponentialBackoff::new(
                Duration::from_millis(50),
                Duration::from_secs(5),
            ));

        assert_eq!(layer.max_attempts, 5);
    }
}
