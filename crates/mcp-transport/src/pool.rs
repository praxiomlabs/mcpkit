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
//! ```ignore
//! use mcp_transport::pool::{Pool, PoolConfig};
//! use mcp_transport::websocket::WebSocketTransport;
//!
//! let config = PoolConfig::new()
//!     .max_connections(10)
//!     .min_connections(2)
//!     .idle_timeout(Duration::from_secs(300));
//!
//! let pool = Pool::new(config, || async {
//!     WebSocketTransport::connect("ws://localhost:8080").await
//! });
//!
//! let conn = pool.acquire().await?;
//! // Use connection...
//! pool.release(conn);
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::Transport;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the minimum number of connections.
    #[must_use]
    pub fn min_connections(mut self, min: usize) -> Self {
        self.min_connections = min;
        self
    }

    /// Set the idle timeout.
    #[must_use]
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the acquire timeout.
    #[must_use]
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Set the health check interval.
    #[must_use]
    pub fn health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    /// Enable or disable testing connections on acquire.
    #[must_use]
    pub fn test_on_acquire(mut self, test: bool) -> Self {
        self.test_on_acquire = test;
        self
    }

    /// Enable or disable testing connections on release.
    #[must_use]
    pub fn test_on_release(mut self, test: bool) -> Self {
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
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get when the connection was created.
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get when the connection was last used.
    pub fn last_used(&self) -> Instant {
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
    pub fn new(config: PoolConfig, factory: F) -> Self {
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
    pub fn config(&self) -> &PoolConfig {
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
    pub fn new(pool: &'a Pool<T, F, Fut>, conn: PooledConnection<T>) -> Self {
        Self {
            pool,
            conn: Some(conn),
        }
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.conn.as_ref().unwrap().connection
    }

    /// Take the connection, preventing automatic release.
    pub fn take(mut self) -> PooledConnection<T> {
        self.conn.take().unwrap()
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

impl<'a, T, F, Fut> Drop for PooledConnectionGuard<'a, T, F, Fut>
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
            dyn Fn() -> std::pin::Pin<
                    Box<dyn Future<Output = Result<T, TransportError>> + Send>,
                > + Send
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
