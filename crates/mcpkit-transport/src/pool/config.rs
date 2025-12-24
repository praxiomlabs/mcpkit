//! Connection pool configuration and statistics types.

use std::time::Duration;

/// Configuration for the connection pool.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// Maximum lifetime for a connection before forced recycling.
    ///
    /// Set to `None` to disable lifetime limits.
    pub max_connection_lifetime: Option<Duration>,
    /// Whether to warm up the pool by pre-creating connections.
    ///
    /// When enabled, `min_connections` will be created on pool initialization.
    pub warm_up: bool,
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

    /// Set the maximum connection lifetime.
    ///
    /// Connections older than this will be recycled even if healthy.
    /// Set to `None` to disable lifetime limits.
    #[must_use]
    pub const fn max_connection_lifetime(mut self, lifetime: Option<Duration>) -> Self {
        self.max_connection_lifetime = lifetime;
        self
    }

    /// Enable or disable pool warm-up.
    ///
    /// When enabled, `min_connections` will be pre-created during pool initialization.
    #[must_use]
    pub const fn warm_up(mut self, enabled: bool) -> Self {
        self.warm_up = enabled;
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
            max_connection_lifetime: None,
            warm_up: false,
        }
    }
}

/// Pool statistics.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
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
    /// Current number of waiters in the queue.
    pub waiters: usize,
    /// Total number of connections recycled due to lifetime limits.
    pub recycled_lifetime: u64,
    /// Total number of connections recycled due to health check failures.
    pub recycled_health: u64,
    /// Peak number of concurrent connections ever used.
    pub peak_in_use: usize,
}

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
