//! Async cancellation tests.
//!
//! Tests verifying correct behavior for request and task cancellation.

use mcpkit_server::context::CancellationToken;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

// =============================================================================
// CancellationToken Basic Tests
// =============================================================================

#[test]
fn test_cancellation_token_creation() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn test_cancellation_token_cancel() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());

    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn test_cancellation_token_double_cancel() {
    let token = CancellationToken::new();

    // Multiple cancellations should be idempotent
    token.cancel();
    token.cancel();
    token.cancel();

    assert!(token.is_cancelled());
}

#[test]
fn test_cancellation_token_default() {
    let token = CancellationToken::default();
    assert!(!token.is_cancelled());
}

// =============================================================================
// CancellationToken Sharing Tests
// =============================================================================

#[test]
fn test_cancellation_token_clone_shares_state() {
    let token1 = CancellationToken::new();
    let token2 = token1.clone();

    assert!(!token1.is_cancelled());
    assert!(!token2.is_cancelled());

    // Cancel through one clone
    token1.cancel();

    // Both should see cancellation
    assert!(token1.is_cancelled());
    assert!(token2.is_cancelled());
}

#[test]
fn test_cancellation_token_cancel_via_clone() {
    let token1 = CancellationToken::new();
    let token2 = token1.clone();

    // Cancel through second clone
    token2.cancel();

    // Both should see cancellation
    assert!(token1.is_cancelled());
    assert!(token2.is_cancelled());
}

// =============================================================================
// CancelledFuture Tests
// =============================================================================

#[tokio::test]
async fn test_cancelled_future_completes_immediately_when_already_cancelled() {
    let token = CancellationToken::new();
    token.cancel();

    // Future should complete immediately
    let result = tokio::time::timeout(Duration::from_millis(10), token.cancelled()).await;

    assert!(
        result.is_ok(),
        "Should complete immediately when already cancelled"
    );
}

#[tokio::test]
async fn test_cancelled_future_waits_for_cancellation() {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    // Spawn a task that will cancel after a delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token_clone.cancel();
    });

    // Wait for cancellation
    let result = tokio::time::timeout(Duration::from_millis(200), token.cancelled()).await;

    assert!(result.is_ok(), "Should complete when cancelled");
    assert!(token.is_cancelled());
}

#[tokio::test]
async fn test_cancelled_future_does_not_complete_if_not_cancelled() {
    let token = CancellationToken::new();

    // Future should not complete within timeout
    let result = tokio::time::timeout(Duration::from_millis(50), token.cancelled()).await;

    assert!(result.is_err(), "Should timeout when not cancelled");
    assert!(!token.is_cancelled());
}

// =============================================================================
// Concurrent Cancellation Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_cancellation_check() {
    let token = CancellationToken::new();
    let check_count = Arc::new(AtomicU32::new(0));

    // Spawn multiple tasks checking cancellation
    let mut handles = vec![];
    for _ in 0..10 {
        let token = token.clone();
        let count = check_count.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                if token.is_cancelled() {
                    break;
                }
                count.fetch_add(1, Ordering::Relaxed);
                tokio::task::yield_now().await;
            }
        }));
    }

    // Cancel after a short delay
    tokio::time::sleep(Duration::from_millis(10)).await;
    token.cancel();

    // Wait for all tasks
    for handle in handles {
        let _ = handle.await;
    }

    // All tasks should have seen the cancellation
    assert!(token.is_cancelled());
    // Some checks should have happened
    assert!(check_count.load(Ordering::Relaxed) > 0);
}

#[tokio::test]
async fn test_concurrent_cancel_operations() {
    let token = CancellationToken::new();

    // Spawn multiple tasks trying to cancel
    let mut handles = vec![];
    for _ in 0..10 {
        let token = token.clone();
        handles.push(tokio::spawn(async move {
            token.cancel();
        }));
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Token should be cancelled
    assert!(token.is_cancelled());
}

// =============================================================================
// Cancellation Propagation Pattern Tests
// =============================================================================

#[tokio::test]
async fn test_cancellation_stops_work() {
    let token = CancellationToken::new();
    let work_done = Arc::new(AtomicU32::new(0));
    let work_done_clone = work_done.clone();

    let token_clone = token.clone();
    let worker = tokio::spawn(async move {
        loop {
            if token_clone.is_cancelled() {
                break;
            }
            work_done_clone.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    // Let some work happen
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Cancel
    token.cancel();

    // Wait for worker to finish
    let _ = tokio::time::timeout(Duration::from_millis(100), worker).await;

    // Some work should have been done before cancellation
    let work = work_done.load(Ordering::Relaxed);
    assert!(
        work > 0,
        "Should have done some work before cancellation: {work}"
    );
    assert!(work < 100, "Should have stopped after cancellation: {work}");
}

#[tokio::test]
async fn test_cancellation_with_select() {
    let token = CancellationToken::new();
    let completed = Arc::new(AtomicU32::new(0));
    let completed_clone = completed.clone();

    let token_clone = token.clone();
    let worker = tokio::spawn(async move {
        tokio::select! {
            () = token_clone.cancelled() => {
                // Cancelled
                return false;
            }
            () = async {
                // Long running work
                tokio::time::sleep(Duration::from_secs(10)).await;
                completed_clone.fetch_add(1, Ordering::Relaxed);
            } => {
                // Completed
                return true;
            }
        }
    });

    // Cancel quickly
    tokio::time::sleep(Duration::from_millis(10)).await;
    token.cancel();

    // Worker should complete quickly via cancellation
    let result = tokio::time::timeout(Duration::from_millis(100), worker).await;
    assert!(result.is_ok(), "Should complete via cancellation");
    assert!(!result.unwrap().unwrap(), "Should indicate cancellation");
    assert_eq!(
        completed.load(Ordering::Relaxed),
        0,
        "Work should not have completed"
    );
}

// =============================================================================
// CancellationToken Send + Sync Tests
// =============================================================================

#[test]
fn test_cancellation_token_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CancellationToken>();
}

// =============================================================================
// CancelledFuture Behavior Tests
// =============================================================================

#[tokio::test]
async fn test_multiple_cancelled_futures_from_same_token() {
    let token = CancellationToken::new();

    // Create multiple futures from the same token
    let fut1 = token.cancelled();
    let fut2 = token.cancelled();
    let fut3 = token.cancelled();

    // Cancel the token
    token.cancel();

    // All should complete
    let result1 = tokio::time::timeout(Duration::from_millis(10), fut1).await;
    let result2 = tokio::time::timeout(Duration::from_millis(10), fut2).await;
    let result3 = tokio::time::timeout(Duration::from_millis(10), fut3).await;

    assert!(result1.is_ok(), "First future should complete");
    assert!(result2.is_ok(), "Second future should complete");
    assert!(result3.is_ok(), "Third future should complete");
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_cancellation_check_is_lock_free() {
    let token = CancellationToken::new();

    // Should be able to check many times quickly
    let mut checks = 0;
    for _ in 0..1_000_000 {
        if token.is_cancelled() {
            break;
        }
        checks += 1;
    }

    assert_eq!(
        checks, 1_000_000,
        "Should complete all checks without blocking"
    );
}

#[tokio::test]
async fn test_cancel_before_future_created() {
    let token = CancellationToken::new();

    // Cancel first
    token.cancel();

    // Then create future
    let fut = token.cancelled();

    // Should complete immediately
    let result = tokio::time::timeout(Duration::from_millis(10), fut).await;
    assert!(
        result.is_ok(),
        "Future created after cancel should complete immediately"
    );
}

// =============================================================================
// Multi-Token Tests
// =============================================================================

#[tokio::test]
async fn test_independent_tokens_dont_affect_each_other() {
    let token1 = CancellationToken::new();
    let token2 = CancellationToken::new();

    assert!(!token1.is_cancelled());
    assert!(!token2.is_cancelled());

    // Cancel only token1
    token1.cancel();

    assert!(token1.is_cancelled());
    assert!(
        !token2.is_cancelled(),
        "token2 should not be affected by token1"
    );
}

// =============================================================================
// Stress Tests
// =============================================================================

#[tokio::test]
async fn test_high_contention_cancellation() {
    let token = CancellationToken::new();
    let check_count = Arc::new(AtomicU32::new(0));
    let cancel_count = Arc::new(AtomicU32::new(0));

    // Spawn many tasks
    let mut handles = vec![];

    // Checkers
    for _ in 0..50 {
        let token = token.clone();
        let count = check_count.clone();
        handles.push(tokio::spawn(async move {
            while !token.is_cancelled() {
                count.fetch_add(1, Ordering::Relaxed);
                tokio::task::yield_now().await;
            }
        }));
    }

    // Cancellers (many trying to cancel)
    for _ in 0..10 {
        let token = token.clone();
        let count = cancel_count.clone();
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            token.cancel();
            count.fetch_add(1, Ordering::Relaxed);
        }));
    }

    // Wait for all
    for handle in handles {
        let _ = handle.await;
    }

    assert!(token.is_cancelled());
    assert_eq!(cancel_count.load(Ordering::Relaxed), 10);
    assert!(check_count.load(Ordering::Relaxed) > 0);
}

// =============================================================================
// Request-Level Cancellation Patterns
// =============================================================================

#[tokio::test]
async fn test_cancellation_in_nested_async_context() {
    let token = CancellationToken::new();

    async fn inner_work(token: &CancellationToken) -> bool {
        for _ in 0..10 {
            if token.is_cancelled() {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        true
    }

    let token_clone = token.clone();
    let handle = tokio::spawn(async move { inner_work(&token_clone).await });

    // Cancel after a short time
    tokio::time::sleep(Duration::from_millis(25)).await;
    token.cancel();

    let result = handle.await.unwrap();
    assert!(!result, "Work should have been cancelled");
}

#[tokio::test]
async fn test_cancellation_race_with_completion() {
    // Test the race between cancellation and normal completion
    let token = CancellationToken::new();
    let completed = Arc::new(AtomicU32::new(0));

    for _ in 0..100 {
        let token = token.clone();
        let completed = completed.clone();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => false,
                _ = tokio::time::sleep(Duration::from_micros(100)) => {
                    completed.fetch_add(1, Ordering::Relaxed);
                    true
                }
            }
        });

        let _ = handle.await;
    }

    // Most should complete before we even try to cancel
    assert!(
        completed.load(Ordering::Relaxed) > 50,
        "Most tasks should complete normally: {}",
        completed.load(Ordering::Relaxed)
    );
}

#[tokio::test]
async fn test_cancellation_token_in_result_chain() {
    let token = CancellationToken::new();

    async fn fallible_work(token: &CancellationToken) -> Result<String, &'static str> {
        if token.is_cancelled() {
            return Err("cancelled");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        if token.is_cancelled() {
            return Err("cancelled");
        }
        Ok("done".to_string())
    }

    // Without cancellation
    let result = fallible_work(&token).await;
    assert!(result.is_ok());

    // With cancellation
    token.cancel();
    let result = fallible_work(&token).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "cancelled");
}

#[tokio::test]
async fn test_cancellation_cleanup_pattern() {
    let token = CancellationToken::new();
    let cleanup_done = Arc::new(AtomicU32::new(0));

    let cleanup = cleanup_done.clone();
    let token_clone = token.clone();
    let handle = tokio::spawn(async move {
        // Simulate resource acquisition
        let _resource = "acquired";

        // Do work with cancellation check
        loop {
            if token_clone.is_cancelled() {
                // Cleanup before returning
                cleanup.fetch_add(1, Ordering::Relaxed);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Cancel after a short time
    tokio::time::sleep(Duration::from_millis(25)).await;
    token.cancel();

    // Wait for task to complete
    let _ = handle.await;

    assert_eq!(
        cleanup_done.load(Ordering::Relaxed),
        1,
        "Cleanup should run"
    );
}
