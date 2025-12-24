//! Connection pool manager implementation.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::error::TransportError;
use crate::runtime::{AsyncMutex, Notify};
use crate::traits::Transport;

use super::config::{PoolConfig, PoolStats};
use super::connection::PooledConnection;

/// Internal pool state.
pub struct PoolState<T> {
    /// Available connections.
    pub available: VecDeque<PooledConnection<T>>,
    /// Number of connections currently in use.
    pub in_use: usize,
    /// Whether the pool is closed.
    pub closed: bool,
    /// Peak number of concurrent connections ever in use.
    pub peak_in_use: usize,
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
    pub(crate) state: AsyncMutex<PoolState<T>>,
    /// Notification for waiters when a connection becomes available.
    notify: Notify,
    next_id: AtomicU64,
    stats_created: AtomicU64,
    stats_closed: AtomicU64,
    stats_acquires: AtomicU64,
    stats_releases: AtomicU64,
    stats_timeouts: AtomicU64,
    /// Number of tasks currently waiting for a connection.
    stats_waiters: AtomicUsize,
    /// Connections recycled due to lifetime limits.
    stats_recycled_lifetime: AtomicU64,
    /// Connections recycled due to health check failures.
    stats_recycled_health: AtomicU64,
}

impl<T, F, Fut> Pool<T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    /// Create a new connection pool.
    #[must_use]
    pub fn new(config: PoolConfig, factory: F) -> Self {
        Self {
            config,
            factory,
            state: AsyncMutex::new(PoolState {
                available: VecDeque::new(),
                in_use: 0,
                closed: false,
                peak_in_use: 0,
            }),
            notify: Notify::new(),
            next_id: AtomicU64::new(1),
            stats_created: AtomicU64::new(0),
            stats_closed: AtomicU64::new(0),
            stats_acquires: AtomicU64::new(0),
            stats_releases: AtomicU64::new(0),
            stats_timeouts: AtomicU64::new(0),
            stats_waiters: AtomicUsize::new(0),
            stats_recycled_lifetime: AtomicU64::new(0),
            stats_recycled_health: AtomicU64::new(0),
        }
    }

    /// Warm up the pool by pre-creating connections.
    ///
    /// Creates connections up to `min_connections` in advance.
    /// This is called automatically if `warm_up` is enabled in config.
    ///
    /// # Errors
    ///
    /// Returns an error if any connection fails to be created.
    pub async fn warm_up(&self) -> Result<(), TransportError> {
        let min_connections = self.config.min_connections;

        for _ in 0..min_connections {
            let state = self.state.lock().await;
            let total = state.available.len() + state.in_use;
            drop(state);

            if total >= min_connections {
                break;
            }

            // Create a new connection
            let connection = (self.factory)().await?;
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);

            self.stats_created.fetch_add(1, Ordering::Relaxed);

            let mut state = self.state.lock().await;
            state
                .available
                .push_back(PooledConnection::new(connection, id));
        }

        Ok(())
    }

    /// Get the pool configuration.
    #[must_use]
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
            waiters: self.stats_waiters.load(Ordering::Relaxed),
            recycled_lifetime: self.stats_recycled_lifetime.load(Ordering::Relaxed),
            recycled_health: self.stats_recycled_health.load(Ordering::Relaxed),
            peak_in_use: state.peak_in_use,
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
                    self.stats_recycled_health.fetch_add(1, Ordering::Relaxed);
                    self.stats_closed.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                // Check idle timeout
                if conn.is_idle(self.config.idle_timeout) {
                    self.stats_closed.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                // Check max connection lifetime
                if let Some(max_lifetime) = self.config.max_connection_lifetime {
                    if conn.is_expired(max_lifetime) {
                        self.stats_recycled_lifetime.fetch_add(1, Ordering::Relaxed);
                        self.stats_closed.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                }

                conn.touch();
                state.in_use += 1;

                // Track peak usage
                if state.in_use > state.peak_in_use {
                    state.peak_in_use = state.in_use;
                }

                self.stats_acquires.fetch_add(1, Ordering::Relaxed);
                return Ok(conn);
            }

            // Check if we can create a new connection
            let total = state.available.len() + state.in_use;
            if total < self.config.max_connections {
                state.in_use += 1;

                // Track peak usage
                if state.in_use > state.peak_in_use {
                    state.peak_in_use = state.in_use;
                }

                drop(state);

                // Create new connection outside the lock
                let connection = (self.factory)().await?;
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);

                self.stats_created.fetch_add(1, Ordering::Relaxed);
                self.stats_acquires.fetch_add(1, Ordering::Relaxed);

                return Ok(PooledConnection::new(connection, id));
            }

            // No connections available and at max capacity - wait for notification
            drop(state);

            // Track waiting
            self.stats_waiters.fetch_add(1, Ordering::Relaxed);

            // Wait for a connection to become available or timeout
            let remaining = self.config.acquire_timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                self.stats_waiters.fetch_sub(1, Ordering::Relaxed);
                self.stats_timeouts.fetch_add(1, Ordering::Relaxed);
                return Err(TransportError::Timeout {
                    operation: "pool acquire".to_string(),
                    duration: self.config.acquire_timeout,
                });
            }

            // Use event notification with timeout for efficient waiting
            let listener = self.notify.listen();
            let wait_result =
                crate::runtime::timeout(remaining.min(Duration::from_millis(100)), listener).await;

            self.stats_waiters.fetch_sub(1, Ordering::Relaxed);

            // Whether we got notified or timed out, try to acquire again
            let _ = wait_result;
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
            // Notify waiters even on close so they can fail fast
            self.notify.notify(1);
            return;
        }

        if self.config.test_on_release && !conn.connection.is_connected() {
            self.stats_recycled_health.fetch_add(1, Ordering::Relaxed);
            self.stats_closed.fetch_add(1, Ordering::Relaxed);
            // Notify waiters so they can try to create a new connection
            self.notify.notify(1);
            return;
        }

        // Check max connection lifetime on release
        if let Some(max_lifetime) = self.config.max_connection_lifetime {
            if conn.is_expired(max_lifetime) {
                self.stats_recycled_lifetime.fetch_add(1, Ordering::Relaxed);
                self.stats_closed.fetch_add(1, Ordering::Relaxed);
                // Notify waiters so they can try to create a new connection
                self.notify.notify(1);
                return;
            }
        }

        conn.touch();
        state.available.push_back(conn);
        self.stats_releases.fetch_add(1, Ordering::Relaxed);

        // Notify one waiter that a connection is available
        self.notify.notify(1);
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

        // Notify all waiters so they can fail fast
        self.notify.notify(usize::MAX);
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
