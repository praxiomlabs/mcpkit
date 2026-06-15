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

/// A guard that frees its pool slot when dropped.
///
/// When the guard is dropped without first calling [`take`](Self::take), the
/// reserved `in_use` slot is released back to the pool so capacity is not
/// leaked. The connection itself is closed rather than returned for reuse,
/// because release in a `Drop` context cannot be async.
///
/// To return a connection to the pool **for reuse**, call [`take`](Self::take)
/// to extract it and pass it to [`Pool::release`] directly.
pub struct PooledConnectionGuard<'a, T, F, Fut>
where
    T: Transport,
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, TransportError>> + Send,
{
    /// Pool the slot belongs to, used to release the slot on drop.
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

    /// Release the reserved pool slot in a synchronous (drop) context.
    ///
    /// We cannot await here to return the connection to the pool's idle set, so
    /// the connection is dropped (closed) and only its `in_use` slot is freed.
    /// If [`take`](Self::take) was already called there is nothing to release.
    fn release_sync(&mut self) {
        if self.conn.take().is_some() {
            self.pool.release_slot();
        }
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
