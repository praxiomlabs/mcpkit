//! Pooled connection wrapper types.

use std::future::Future;
use std::time::{Duration, Instant};

use crate::error::TransportError;
use crate::traits::Transport;

use super::manager::Pool;

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
    pub(crate) fn new(connection: T, id: u64) -> Self {
        let now = Instant::now();
        Self {
            connection,
            created_at: now,
            last_used: now,
            id,
        }
    }

    /// Get the connection ID.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Get when the connection was created.
    #[must_use]
    pub const fn created_at(&self) -> Instant {
        self.created_at
    }

    /// Get when the connection was last used.
    #[must_use]
    pub const fn last_used(&self) -> Instant {
        self.last_used
    }

    /// Mark the connection as used now.
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Check if the connection has been idle longer than the timeout.
    #[must_use]
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() > timeout
    }

    /// Check if the connection has exceeded its maximum lifetime.
    #[must_use]
    pub fn is_expired(&self, max_lifetime: Duration) -> bool {
        self.created_at.elapsed() > max_lifetime
    }

    /// Get the age of the connection.
    #[must_use]
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pooled_connection() {
        let conn = PooledConnection::new((), 1);

        assert_eq!(conn.id(), 1);
        assert!(!conn.is_idle(Duration::from_secs(60)));
    }
}
