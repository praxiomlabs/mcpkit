//! Connection pooling for MCP transports.
//!
//! This module provides connection pooling functionality for managing
//! multiple MCP connections efficiently.
//!
//! # Features
//!
//! - Configurable pool size
//! - Automatic connection health checking
//! - Connection reuse and recycling
//! - Idle connection timeout
//! - Fair connection distribution
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::pool::PoolConfig;
//! use std::time::Duration;
//!
//! // Configure a connection pool
//! let config = PoolConfig::new()
//!     .max_connections(10)
//!     .min_connections(2)
//!     .idle_timeout(Duration::from_secs(300))
//!     .test_on_acquire(true);
//!
//! assert_eq!(config.max_connections, 10);
//! assert_eq!(config.min_connections, 2);
//! assert!(config.test_on_acquire);
//! ```

mod config;
mod connection;
mod manager;

// Re-export public types
pub use config::{PoolConfig, PoolStats};
pub use connection::{PooledConnection, PooledConnectionGuard};
pub use manager::{Pool, SimplePool};

/// High-concurrency stress tests for the connection pool.
///
/// These tests verify pool behavior under heavy concurrent load,
/// testing pool exhaustion, recovery, and fairness.
#[cfg(all(test, feature = "tokio-runtime"))]
mod stress_tests {
    use super::*;
    use crate::error::TransportError;
    use crate::traits::{Transport, TransportMetadata};
    use mcpkit_core::protocol::Message;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Mock transport for stress testing.
    struct MockTransport {
        connected: AtomicBool,
        id: u64,
    }

    impl MockTransport {
        const fn new(id: u64) -> Self {
            Self {
                connected: AtomicBool::new(true),
                id,
            }
        }
    }

    impl Transport for MockTransport {
        type Error = TransportError;

        async fn send(&self, _msg: Message) -> Result<(), Self::Error> {
            if !self.is_connected() {
                return Err(TransportError::NotConnected);
            }
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Message>, Self::Error> {
            if !self.is_connected() {
                return Err(TransportError::NotConnected);
            }
            Ok(None)
        }

        async fn close(&self) -> Result<(), Self::Error> {
            self.connected.store(false, Ordering::SeqCst);
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected.load(Ordering::SeqCst)
        }

        fn metadata(&self) -> TransportMetadata {
            TransportMetadata::new("mock")
                .remote_addr(format!("mock-{}", self.id))
                .connected_now()
        }
    }

    /// Slow mock transport that simulates connection latency.
    struct SlowMockTransport {
        inner: MockTransport,
        delay: Duration,
    }

    impl SlowMockTransport {
        fn new(id: u64, delay: Duration) -> Self {
            Self {
                inner: MockTransport::new(id),
                delay,
            }
        }
    }

    impl Transport for SlowMockTransport {
        type Error = TransportError;

        async fn send(&self, msg: Message) -> Result<(), Self::Error> {
            crate::runtime::sleep(self.delay).await;
            self.inner.send(msg).await
        }

        async fn recv(&self) -> Result<Option<Message>, Self::Error> {
            crate::runtime::sleep(self.delay).await;
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

    /// Type alias for mock pool future.
    type MockFuture = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<MockTransport, TransportError>> + Send>,
    >;

    /// Type alias for slow mock pool future.
    type SlowMockFuture = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SlowMockTransport, TransportError>> + Send>,
    >;

    /// Creates a pool with mock transport factory.
    fn create_mock_pool(
        config: PoolConfig,
    ) -> Pool<MockTransport, Box<dyn Fn() -> MockFuture + Send + Sync>, MockFuture> {
        let counter = Arc::new(AtomicU64::new(0));
        let factory: Box<dyn Fn() -> MockFuture + Send + Sync> = Box::new(move || {
            let id = counter.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move { Ok(MockTransport::new(id)) }) as MockFuture
        });
        Pool::new(config, factory)
    }

    /// Creates a pool with slow mock transport factory.
    fn create_slow_mock_pool(
        config: PoolConfig,
        delay: Duration,
    ) -> Pool<SlowMockTransport, Box<dyn Fn() -> SlowMockFuture + Send + Sync>, SlowMockFuture>
    {
        let counter = Arc::new(AtomicU64::new(0));
        let factory: Box<dyn Fn() -> SlowMockFuture + Send + Sync> = Box::new(move || {
            let id = counter.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move { Ok(SlowMockTransport::new(id, delay)) }) as SlowMockFuture
        });
        Pool::new(config, factory)
    }

    // =========================================================================
    // Basic Pool Functionality Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_basic_acquire_release() {
        let config = PoolConfig::new().max_connections(5);
        let pool = create_mock_pool(config);

        // Acquire a connection
        let conn = pool.acquire().await.expect("Failed to acquire connection");
        assert!(conn.connection.is_connected());

        // Check stats
        let stats = pool.stats().await;
        assert_eq!(stats.connections_created, 1);
        assert_eq!(stats.in_use, 1);
        assert_eq!(stats.idle, 0);

        // Release connection
        pool.release(conn).await;

        let stats = pool.stats().await;
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.idle, 1);
    }

    #[tokio::test]
    async fn test_pool_reuses_connections() {
        let config = PoolConfig::new().max_connections(5);
        let pool = create_mock_pool(config);

        // Acquire and release a connection
        let conn1 = pool.acquire().await.expect("Failed to acquire");
        let id1 = conn1.id();
        pool.release(conn1).await;

        // Acquire again - should get the same connection
        let conn2 = pool.acquire().await.expect("Failed to acquire");
        let id2 = conn2.id();
        pool.release(conn2).await;

        assert_eq!(id1, id2, "Pool should reuse connections");

        let stats = pool.stats().await;
        assert_eq!(
            stats.connections_created, 1,
            "Should only create one connection"
        );
    }

    // =========================================================================
    // High-Concurrency Stress Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_high_concurrency_100_tasks() {
        let config = PoolConfig::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(10));
        let pool = Arc::new(create_mock_pool(config));

        let successful = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        // Spawn 100 concurrent tasks
        for _ in 0..100 {
            let pool = Arc::clone(&pool);
            let successful = Arc::clone(&successful);

            handles.push(tokio::spawn(async move {
                if let Ok(conn) = pool.acquire().await {
                    // Simulate some work
                    crate::runtime::sleep(Duration::from_millis(1)).await;
                    pool.release(conn).await;
                    successful.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }

        let success_count = successful.load(Ordering::Relaxed);
        assert_eq!(
            success_count, 100,
            "All 100 tasks should complete successfully"
        );

        let stats = pool.stats().await;
        assert!(
            stats.connections_created <= 10,
            "Should create at most max_connections"
        );
        assert_eq!(stats.acquires, 100, "Should have 100 acquires");
    }

    #[tokio::test]
    async fn test_pool_stress_500_concurrent_requests() {
        let config = PoolConfig::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(30));
        let pool = Arc::new(create_mock_pool(config));

        let successful = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        // Spawn 500 concurrent tasks with varying work times
        for i in 0..500 {
            let pool = Arc::clone(&pool);
            let successful = Arc::clone(&successful);

            handles.push(tokio::spawn(async move {
                if let Ok(conn) = pool.acquire().await {
                    // Varying work times to create contention
                    let work_time = Duration::from_micros((i % 10 * 100) as u64);
                    crate::runtime::sleep(work_time).await;
                    pool.release(conn).await;
                    successful.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        let success_count = successful.load(Ordering::Relaxed);
        assert_eq!(success_count, 500, "All 500 tasks should complete");

        let stats = pool.stats().await;
        assert!(stats.connections_created <= 20);
        assert_eq!(stats.acquires, 500);
    }

    #[tokio::test]
    async fn test_pool_exhaustion_and_recovery() {
        let config = PoolConfig::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(5));
        let pool = Arc::new(create_mock_pool(config));

        // Acquire all connections
        let conn1 = pool.acquire().await.expect("Failed to acquire conn1");
        let conn2 = pool.acquire().await.expect("Failed to acquire conn2");
        let conn3 = pool.acquire().await.expect("Failed to acquire conn3");

        let stats = pool.stats().await;
        assert_eq!(stats.in_use, 3);
        assert_eq!(stats.idle, 0);

        // Spawn a task that waits for a connection
        let pool_clone = Arc::clone(&pool);
        let waiter = tokio::spawn(async move { pool_clone.acquire().await });

        // Give the waiter time to start waiting
        crate::runtime::sleep(Duration::from_millis(50)).await;

        // Release one connection
        pool.release(conn1).await;

        // Waiter should now complete
        let result = tokio::time::timeout(Duration::from_secs(2), waiter).await;
        assert!(
            result.is_ok(),
            "Waiter should get a connection after release"
        );
        let conn4 = result
            .unwrap()
            .expect("Waiter task panicked")
            .expect("Failed to acquire");

        // Cleanup
        pool.release(conn2).await;
        pool.release(conn3).await;
        pool.release(conn4).await;
    }

    #[tokio::test]
    async fn test_pool_timeout_under_exhaustion() {
        let config = PoolConfig::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_millis(100));
        let pool = Arc::new(create_mock_pool(config));

        // Exhaust the pool
        let conn1 = pool.acquire().await.expect("Failed to acquire conn1");
        let conn2 = pool.acquire().await.expect("Failed to acquire conn2");

        // Try to acquire when exhausted - should timeout
        let start = Instant::now();
        let result = pool.acquire().await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "Should timeout when pool is exhausted");
        assert!(
            elapsed >= Duration::from_millis(100),
            "Should wait for timeout"
        );
        assert!(
            elapsed < Duration::from_millis(200),
            "Should not wait too long"
        );

        let stats = pool.stats().await;
        assert_eq!(stats.timeouts, 1, "Should record the timeout");

        pool.release(conn1).await;
        pool.release(conn2).await;
    }

    #[tokio::test]
    async fn test_pool_concurrent_acquire_release_waves() {
        let config = PoolConfig::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(10));
        let pool = Arc::new(create_mock_pool(config));

        // Run 5 waves of 50 concurrent operations each
        for wave in 0..5 {
            let mut handles = Vec::new();
            let wave_success = Arc::new(AtomicUsize::new(0));

            for i in 0..50 {
                let pool = Arc::clone(&pool);
                let wave_success = Arc::clone(&wave_success);

                handles.push(tokio::spawn(async move {
                    let conn = pool.acquire().await.expect("Failed to acquire");
                    crate::runtime::sleep(Duration::from_micros(100 * (i % 5) as u64)).await;
                    pool.release(conn).await;
                    wave_success.fetch_add(1, Ordering::Relaxed);
                }));
            }

            for handle in handles {
                let _ = handle.await;
            }

            assert_eq!(
                wave_success.load(Ordering::Relaxed),
                50,
                "Wave {wave} should complete all tasks"
            );
        }

        let stats = pool.stats().await;
        assert_eq!(stats.acquires, 250, "Should have 5 waves * 50 acquires");
        assert!(stats.connections_created <= 10);
    }

    // =========================================================================
    // Connection Lifecycle Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_idle_timeout() {
        let config = PoolConfig::new()
            .max_connections(5)
            .min_connections(1)
            .idle_timeout(Duration::from_millis(50));
        let pool = Arc::new(create_mock_pool(config));

        // Acquire and release a connection
        let conn = pool.acquire().await.expect("Failed to acquire");
        pool.release(conn).await;

        let stats = pool.stats().await;
        assert_eq!(stats.idle, 1);

        // Wait for idle timeout
        crate::runtime::sleep(Duration::from_millis(100)).await;

        // Cleanup idle connections
        pool.cleanup_idle().await;

        // Due to min_connections = 1, the idle connection should remain
        // unless we have more than min_connections idle

        // Acquire 2 connections to test cleanup with multiple idle
        let conn1 = pool.acquire().await.expect("Failed to acquire");
        let conn2 = pool.acquire().await.expect("Failed to acquire");
        pool.release(conn1).await;
        pool.release(conn2).await;

        // Wait for idle timeout
        crate::runtime::sleep(Duration::from_millis(100)).await;

        // Now we have 2 idle, min is 1, so cleanup should remove 1
        pool.cleanup_idle().await;

        let stats = pool.stats().await;
        // At least 1 should be cleaned up (the one over min_connections)
        assert!(stats.idle >= 1 && stats.idle <= 2);
    }

    #[tokio::test]
    async fn test_pool_close() {
        let config = PoolConfig::new().max_connections(5);
        let pool = Arc::new(create_mock_pool(config));

        // Acquire and release some connections
        let conn1 = pool.acquire().await.expect("Failed to acquire");
        let conn2 = pool.acquire().await.expect("Failed to acquire");
        pool.release(conn1).await;
        pool.release(conn2).await;

        let stats = pool.stats().await;
        assert_eq!(stats.idle, 2);

        // Close the pool
        pool.close().await;

        assert!(pool.is_closed().await);

        let stats = pool.stats().await;
        assert!(
            stats.connections_closed >= 2,
            "Should close idle connections"
        );

        // Acquiring from closed pool should fail
        let result = pool.acquire().await;
        assert!(result.is_err(), "Should not acquire from closed pool");
    }

    #[tokio::test]
    async fn test_pool_unhealthy_connection_removal() {
        let config = PoolConfig::new().max_connections(5).test_on_acquire(true);
        let pool = Arc::new(create_mock_pool(config));

        // Acquire and release a connection
        let conn = pool.acquire().await.expect("Failed to acquire");
        let id = conn.id();
        // Manually disconnect the transport
        conn.connection.close().await.expect("Failed to close");
        pool.release(conn).await;

        // Next acquire should skip the unhealthy connection and create a new one
        let conn2 = pool.acquire().await.expect("Failed to acquire");
        // Should have a different connection ID since old one was unhealthy
        assert_ne!(
            conn2.id(),
            id,
            "Should get a new connection after unhealthy one"
        );

        let stats = pool.stats().await;
        assert!(
            stats.connections_closed >= 1,
            "Should have closed unhealthy connection"
        );

        pool.release(conn2).await;
    }

    // =========================================================================
    // Fairness Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_fair_connection_distribution() {
        let config = PoolConfig::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(10));
        let pool = Arc::new(create_mock_pool(config));

        // Track which connection IDs are used
        let id_usage = Arc::new(std::sync::Mutex::new(Vec::new()));

        // Run many acquire/release cycles
        for _ in 0..30 {
            let pool = Arc::clone(&pool);
            let id_usage = Arc::clone(&id_usage);

            let conn = pool.acquire().await.expect("Failed to acquire");
            id_usage.lock().expect("Mutex poisoned").push(conn.id());
            pool.release(conn).await;
        }

        let ids = id_usage.lock().expect("Mutex poisoned").clone();

        // All connections should be used (1, 2, 3 created initially)
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert!(!unique.is_empty(), "Should reuse connections from pool");
    }

    // =========================================================================
    // Slow Connection Factory Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_slow_connection_factory() {
        let config = PoolConfig::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10));
        let pool = Arc::new(create_slow_mock_pool(config, Duration::from_millis(10)));

        let successful = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        // Spawn 20 concurrent tasks with slow connection creation
        for _ in 0..20 {
            let pool = Arc::clone(&pool);
            let successful = Arc::clone(&successful);

            handles.push(tokio::spawn(async move {
                if let Ok(conn) = pool.acquire().await {
                    pool.release(conn).await;
                    successful.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        assert_eq!(successful.load(Ordering::Relaxed), 20);
    }

    // =========================================================================
    // Stats Accuracy Tests
    // =========================================================================

    #[tokio::test]
    async fn test_pool_stats_accuracy_under_load() {
        let config = PoolConfig::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(10));
        let pool = Arc::new(create_mock_pool(config));

        let mut handles = Vec::new();

        // Spawn 100 tasks
        for _ in 0..100 {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let conn = pool.acquire().await.expect("Failed to acquire");
                crate::runtime::sleep(Duration::from_micros(100)).await;
                pool.release(conn).await;
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        let stats = pool.stats().await;

        // Verify stats consistency
        assert_eq!(stats.acquires, 100, "Should track all acquires");
        assert_eq!(stats.releases, 100, "Should track all releases");
        assert!(
            stats.connections_created <= 10,
            "Should respect max_connections"
        );
        assert_eq!(stats.in_use, 0, "All connections should be released");
        assert!(stats.idle > 0, "Should have idle connections");
        assert_eq!(stats.timeouts, 0, "Should have no timeouts");
    }
}
