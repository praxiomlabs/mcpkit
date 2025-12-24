//! Async testing utilities.
//!
//! This module provides helpers for testing async MCP code,
//! including timeout wrappers and assertion helpers.

use std::future::Future;
use std::time::Duration;

/// Default timeout for async operations in tests.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Run an async function with a timeout.
///
/// # Panics
///
/// Panics if the future does not complete within the timeout.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_testing::async_helpers::with_timeout;
/// use std::time::Duration;
///
/// #[tokio::test]
/// async fn test_with_timeout() {
///     let result = with_timeout(Duration::from_secs(1), async {
///         "hello"
///     }).await;
///     assert_eq!(result, "hello");
/// }
/// ```
pub async fn with_timeout<T, F>(timeout: Duration, future: F) -> T
where
    F: Future<Output = T>,
{
    tokio::time::timeout(timeout, future)
        .await
        .expect("Test timed out")
}

/// Run an async function with the default timeout.
///
/// Uses [`DEFAULT_TIMEOUT`] (5 seconds) as the timeout.
pub async fn with_default_timeout<T, F>(future: F) -> T
where
    F: Future<Output = T>,
{
    with_timeout(DEFAULT_TIMEOUT, future).await
}

/// Assert that an async operation completes within a timeout.
///
/// # Panics
///
/// Panics if the future does not complete within the timeout.
pub async fn assert_completes_within<T, F>(timeout: Duration, future: F) -> T
where
    F: Future<Output = T>,
{
    tokio::time::timeout(timeout, future)
        .await
        .expect("Operation did not complete within timeout")
}

/// Assert that an async operation times out.
///
/// # Panics
///
/// Panics if the future completes before the timeout.
pub async fn assert_times_out<T, F>(timeout: Duration, future: F)
where
    F: Future<Output = T>,
{
    let result = tokio::time::timeout(timeout, future).await;
    assert!(
        result.is_err(),
        "Expected operation to timeout, but it completed"
    );
}

/// Wait for a condition to become true.
///
/// Polls the condition function at regular intervals until it returns true
/// or the timeout is reached.
///
/// # Panics
///
/// Panics if the condition is not met within the timeout.
pub async fn wait_for<F>(timeout: Duration, interval: Duration, mut condition: F)
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while !condition() {
        assert!(
            start.elapsed() <= timeout,
            "Condition not met within timeout"
        );
        tokio::time::sleep(interval).await;
    }
}

/// Wait for an async condition to become true.
///
/// # Panics
///
/// Panics if the condition is not met within the timeout.
pub async fn wait_for_async<F, Fut>(timeout: Duration, interval: Duration, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = std::time::Instant::now();
    loop {
        if condition().await {
            return;
        }
        assert!(
            start.elapsed() <= timeout,
            "Condition not met within timeout"
        );
        tokio::time::sleep(interval).await;
    }
}

/// Retry an async operation until it succeeds or max attempts is reached.
///
/// # Errors
///
/// Returns the last error if all attempts fail.
pub async fn retry<T, E, F, Fut>(
    max_attempts: usize,
    delay: Duration,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut last_error = None;

    for attempt in 0..max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts - 1 {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.expect("At least one attempt should have been made"))
}

/// A test barrier for synchronizing async tests.
///
/// Useful for coordinating between client and server in integration tests.
#[derive(Debug)]
pub struct TestBarrier {
    notify: tokio::sync::Notify,
    count: std::sync::atomic::AtomicUsize,
    target: usize,
}

impl TestBarrier {
    /// Create a new barrier with the specified target count.
    #[must_use]
    pub fn new(target: usize) -> Self {
        Self {
            notify: tokio::sync::Notify::new(),
            count: std::sync::atomic::AtomicUsize::new(0),
            target,
        }
    }

    /// Arrive at the barrier and wait for all parties.
    pub async fn arrive_and_wait(&self) {
        let count = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if count >= self.target {
            self.notify.notify_waiters();
        } else {
            self.notify.notified().await;
        }
    }

    /// Reset the barrier for reuse.
    pub fn reset(&self) {
        self.count.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A test latch that can be awaited once.
#[derive(Debug, Default)]
pub struct TestLatch {
    notify: tokio::sync::Notify,
    triggered: std::sync::atomic::AtomicBool,
}

impl TestLatch {
    /// Create a new latch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Trigger the latch.
    pub fn trigger(&self) {
        self.triggered
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Wait for the latch to be triggered.
    pub async fn wait(&self) {
        if self.triggered.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.notify.notified().await;
    }

    /// Wait for the latch with a timeout.
    pub async fn wait_timeout(&self, timeout: Duration) -> bool {
        if self.triggered.load(std::sync::atomic::Ordering::SeqCst) {
            return true;
        }
        tokio::time::timeout(timeout, self.notify.notified())
            .await
            .is_ok()
    }

    /// Check if the latch has been triggered.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Collect async stream items into a vector with timeout.
///
/// # Panics
///
/// Panics if the collection times out.
pub async fn collect_with_timeout<S, T>(timeout: Duration, mut stream: S) -> Vec<T>
where
    S: futures::Stream<Item = T> + Unpin,
{
    use futures::StreamExt;

    let mut items = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(item)) => items.push(item),
            Ok(None) => break,
            Err(_) => break,
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_with_timeout_success() {
        let result = with_timeout(Duration::from_secs(1), async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    #[should_panic(expected = "timed out")]
    async fn test_with_timeout_failure() {
        with_timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_secs(10)).await;
        })
        .await;
    }

    #[tokio::test]
    async fn test_wait_for() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = counter.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            counter_clone.store(5, std::sync::atomic::Ordering::SeqCst);
        });

        wait_for(Duration::from_secs(1), Duration::from_millis(10), || {
            counter.load(std::sync::atomic::Ordering::SeqCst) >= 5
        })
        .await;

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_retry_success() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str, &str> = retry(3, Duration::from_millis(10), || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 2 {
                    Err("not yet")
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_test_latch() {
        let latch = std::sync::Arc::new(TestLatch::new());
        let latch_clone = latch.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            latch_clone.trigger();
        });

        assert!(!latch.is_triggered());
        latch.wait().await;
        assert!(latch.is_triggered());

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_test_barrier() {
        let barrier = std::sync::Arc::new(TestBarrier::new(2));
        let barrier_clone = barrier.clone();

        let handle = tokio::spawn(async move {
            barrier_clone.arrive_and_wait().await;
            "done"
        });

        barrier.arrive_and_wait().await;
        let result = handle.await.unwrap();
        assert_eq!(result, "done");
    }
}
