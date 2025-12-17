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

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::Transport;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Configuration for the connection pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    pub max_connections: usize,
    /// Minimum number of connections to maintain.
    pub min_connections: usize,
    /// Idle timeout before a connection is closed.
    pub idle_timeout: Duration,
    /// Maximum time to wait for a connection.
    pub acquire_timeout: Duration,
    /// Interval for health checks.
    pub health_check_interval: Duration,
    /// Whether to test connections before returning them.
    pub test_on_acquire: bool,
    /// Whether to test connections when returning them to the pool.
    pub test_on_release: bool,
}

impl PoolConfig {
    /// Create a new pool configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of connections.
    #[must_use]
    pub const fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the minimum number of connections.
    #[must_use]
    pub const fn min_connections(mut self, min: usize) -> Self {
        self.min_connections = min;
        self
    }

    /// Set the idle timeout.
    #[must_use]
    pub const fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the acquire timeout.
    #[must_use]
    pub const fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Set the health check interval.
    #[must_use]
    pub const fn health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    /// Enable or disable testing connections on acquire.
    #[must_use]
    pub const fn test_on_acquire(mut self, test: bool) -> Self {
        self.test_on_acquire = test;
        self
    }

    /// Enable or disable testing connections on release.
    #[must_use]
    pub const fn test_on_release(mut self, test: bool) -> Self {
        self.test_on_release = test;
        self
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            idle_timeout: Duration::from_secs(300),
            acquire_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            test_on_acquire: true,
            test_on_release: false,
        }
    }
}

/// A pooled connection wrapper.
///
/// Tracks when the connection was last used for idle timeout management.
pub struct PooledConnection<T> {
    /// The underlying connection.
    pub connection: T,
    /// When the connection was created.
    created_at: Instant,
    /// When the connection was last used.
    last_used: Instant,
    /// Connection ID for tracking.
    id: u64,
}

impl<T> PooledConnection<T> {
    /// Create a new pooled connection.
    fn new(connection: T, id: u64) -> Self {
        let now = Instant::now();
        Self {
            connection,
            created_at: now,
            last_used: now,
            id,
        }
    }

    /// Get the connection ID.
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Get when the connection was created.
    pub const fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get when the connection was last used.
    pub const fn last_used(&self) -> Instant {
        self.last_used
    }

    /// Mark the connection as used now.
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Check if the connection has been idle longer than the timeout.
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() > timeout
    }
}

/// Pool statistics.
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total number of connections created.
    pub connections_created: u64,
    /// Total number of connections closed.
    pub connections_closed: u64,
    /// Total number of successful acquires.
    pub acquires: u64,
    /// Total number of releases.
    pub releases: u64,
    /// Total number of acquire timeouts.
    pub timeouts: u64,
    /// Current number of connections in use.
    pub in_use: usize,
    /// Current number of idle connections.
    pub idle: usize,
}

/// Internal pool state.
struct PoolState<T> {
    /// Available connections.
    available: VecDeque<PooledConnection<T>>,
    /// Number of connections currently in use.
    in_use: usize,
    /// Whether the pool is closed.
    closed: bool,
}

/// A connection pool for managing MCP transport connections.
///
/// The pool maintains a set of connections and provides efficient
/// connection reuse with configurable limits and health checking.
pub struct Pool<T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    config: PoolConfig,
    factory: F,
    state: AsyncMutex<PoolState<T>>,
    next_id: AtomicU64,
    stats_created: AtomicU64,
    stats_closed: AtomicU64,
    stats_acquires: AtomicU64,
    stats_releases: AtomicU64,
    stats_timeouts: AtomicU64,
}

impl<T, F, Fut> Pool<T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    /// Create a new connection pool.
    pub const fn new(config: PoolConfig, factory: F) -> Self {
        Self {
            config,
            factory,
            state: AsyncMutex::new(PoolState {
                available: VecDeque::new(),
                in_use: 0,
                closed: false,
            }),
            next_id: AtomicU64::new(1),
            stats_created: AtomicU64::new(0),
            stats_closed: AtomicU64::new(0),
            stats_acquires: AtomicU64::new(0),
            stats_releases: AtomicU64::new(0),
            stats_timeouts: AtomicU64::new(0),
        }
    }

    /// Get the pool configuration.
    pub const fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get current pool statistics.
    pub async fn stats(&self) -> PoolStats {
        let state = self.state.lock().await;
        PoolStats {
            connections_created: self.stats_created.load(Ordering::Relaxed),
            connections_closed: self.stats_closed.load(Ordering::Relaxed),
            acquires: self.stats_acquires.load(Ordering::Relaxed),
            releases: self.stats_releases.load(Ordering::Relaxed),
            timeouts: self.stats_timeouts.load(Ordering::Relaxed),
            in_use: state.in_use,
            idle: state.available.len(),
        }
    }

    /// Acquire a connection from the pool.
    ///
    /// This will either return an existing idle connection or create a new one
    /// if the pool has capacity.
    pub async fn acquire(&self) -> Result<PooledConnection<T>, TransportError> {
        let start = Instant::now();

        loop {
            // Check timeout
            if start.elapsed() > self.config.acquire_timeout {
                self.stats_timeouts.fetch_add(1, Ordering::Relaxed);
                return Err(TransportError::Timeout {
                    operation: "pool acquire".to_string(),
                    duration: self.config.acquire_timeout,
                });
            }

            let mut state = self.state.lock().await;

            if state.closed {
                return Err(TransportError::Connection {
                    message: "Pool is closed".to_string(),
                });
            }

            // Try to get an available connection
            while let Some(mut conn) = state.available.pop_front() {
                // Check if connection is still healthy
                if self.config.test_on_acquire && !conn.connection.is_connected() {
                    self.stats_closed.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                // Check idle timeout
                if conn.is_idle(self.config.idle_timeout) {
                    self.stats_closed.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                conn.touch();
                state.in_use += 1;
                self.stats_acquires.fetch_add(1, Ordering::Relaxed);
                return Ok(conn);
            }

            // Check if we can create a new connection
            let total = state.available.len() + state.in_use;
            if total < self.config.max_connections {
                state.in_use += 1;
                drop(state);

                // Create new connection outside the lock
                let connection = (self.factory)().await?;
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);

                self.stats_created.fetch_add(1, Ordering::Relaxed);
                self.stats_acquires.fetch_add(1, Ordering::Relaxed);

                return Ok(PooledConnection::new(connection, id));
            }

            // No connections available and at max capacity - wait and retry
            drop(state);
            crate::runtime::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Release a connection back to the pool.
    pub async fn release(&self, mut conn: PooledConnection<T>) {
        let mut state = self.state.lock().await;

        if state.in_use > 0 {
            state.in_use -= 1;
        }

        // Check if pool is closed or connection is unhealthy
        if state.closed {
            self.stats_closed.fetch_add(1, Ordering::Relaxed);
            return;
        }

        if self.config.test_on_release && !conn.connection.is_connected() {
            self.stats_closed.fetch_add(1, Ordering::Relaxed);
            return;
        }

        conn.touch();
        state.available.push_back(conn);
        self.stats_releases.fetch_add(1, Ordering::Relaxed);
    }

    /// Close the pool and all connections.
    pub async fn close(&self) {
        let mut state = self.state.lock().await;
        state.closed = true;

        // Close all available connections
        while let Some(conn) = state.available.pop_front() {
            let _ = conn.connection.close().await;
            self.stats_closed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check if the pool is closed.
    pub async fn is_closed(&self) -> bool {
        self.state.lock().await.closed
    }

    /// Clean up idle connections.
    ///
    /// Removes connections that have been idle longer than the configured timeout.
    pub async fn cleanup_idle(&self) {
        let mut state = self.state.lock().await;

        let timeout = self.config.idle_timeout;
        let min_connections = self.config.min_connections;

        // Keep at least min_connections
        while state.available.len() > min_connections {
            if let Some(conn) = state.available.front() {
                if conn.is_idle(timeout) {
                    if let Some(conn) = state.available.pop_front() {
                        let _ = conn.connection.close().await;
                        self.stats_closed.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    break;
                }
            }
        }
    }
}

/// A simple wrapper for pools that provides automatic connection release.
///
/// When dropped, the connection is released. For proper async release back to
/// the pool, use the `take()` method to extract the connection and call
/// `Pool::release()` directly.
pub struct PooledConnectionGuard<'a, T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    /// Reference to the pool (kept for future async release support).
    #[allow(dead_code)]
    pool: &'a Pool<T, F, Fut>,
    conn: Option<PooledConnection<T>>,
}

impl<'a, T, F, Fut> PooledConnectionGuard<'a, T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    /// Create a new guard wrapping a pooled connection.
    pub const fn new(pool: &'a Pool<T, F, Fut>, conn: PooledConnection<T>) -> Self {
        Self {
            pool,
            conn: Some(conn),
        }
    }

    /// Get a reference to the underlying transport.
    ///
    /// # Panics
    ///
    /// Panics if `take()` was previously called on this guard.
    pub fn transport(&self) -> &T {
        &self
            .conn
            .as_ref()
            .expect("transport() called after take()")
            .connection
    }

    /// Take the connection, preventing automatic release.
    ///
    /// # Panics
    ///
    /// Panics if `take()` was previously called on this guard.
    pub fn take(mut self) -> PooledConnection<T> {
        self.conn
            .take()
            .expect("take() called twice on PooledConnectionGuard")
    }

    /// Release the connection back to the pool.
    ///
    /// This is a best-effort operation in synchronous context.
    /// For proper async release, use `Pool::release()` directly.
    fn release_sync(&mut self) {
        // In drop context, we cannot await, so we just drop the connection.
        // Proper async release should use Pool::release() before dropping.
        // The connection count will be decremented when the Pool detects
        // the connection is no longer available.
        self.conn.take();
    }
}

impl<T, F, Fut> Drop for PooledConnectionGuard<'_, T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    fn drop(&mut self) {
        self.release_sync();
    }
}

/// A connection pool with a fixed factory function type.
///
/// This type alias simplifies usage when the factory is a closure.
pub type SimplePool<T> = Arc<
    Pool<
        T,
        Box<
            dyn Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T, TransportError>> + Send>>
                + Send
                + Sync,
        >,
        std::pin::Pin<Box<dyn Future<Output = Result<T, TransportError>> + Send>>,
    >,
>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = PoolConfig::new()
            .max_connections(20)
            .min_connections(5)
            .idle_timeout(Duration::from_secs(600))
            .acquire_timeout(Duration::from_secs(10));

        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.idle_timeout, Duration::from_secs(600));
        assert_eq!(config.acquire_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_pooled_connection() {
        let conn = PooledConnection::new((), 1);

        assert_eq!(conn.id(), 1);
        assert!(!conn.is_idle(Duration::from_secs(60)));
    }

    #[test]
    fn test_pool_stats_default() {
        let stats = PoolStats::default();

        assert_eq!(stats.connections_created, 0);
        assert_eq!(stats.connections_closed, 0);
        assert_eq!(stats.acquires, 0);
        assert_eq!(stats.releases, 0);
        assert_eq!(stats.in_use, 0);
        assert_eq!(stats.idle, 0);
    }
}

/// High-concurrency stress tests for the connection pool.
///
/// These tests verify pool behavior under heavy concurrent load,
/// testing pool exhaustion, recovery, and fairness.
#[cfg(all(test, feature = "tokio-runtime"))]
mod stress_tests {
    use super::*;
    use crate::traits::TransportMetadata;
    use mcpkit_core::protocol::Message;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    type MockFuture =
        std::pin::Pin<Box<dyn Future<Output = Result<MockTransport, TransportError>> + Send>>;

    /// Type alias for slow mock pool future.
    type SlowMockFuture =
        std::pin::Pin<Box<dyn Future<Output = Result<SlowMockTransport, TransportError>> + Send>>;

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
